//! End-to-end test for the live WebSocket Nostr client against a **local,
//! in-process Nostr relay** — real WebSocket frames, real signed NIP-01 events,
//! real relay filter matching — with no external network.
//!
//! Two Lifeline nodes' Nostr clients connect to the relay; one broadcasts a
//! discovery beacon, the other learns it as a peer and replies with a directed
//! frame that the relay routes back by `#p` tag. This exercises the same path a
//! node would use against `wss://relay.damus.io`, just over `ws://127.0.0.1`.
#![cfg(feature = "ws")]

use lifeline_bridge::nostr::{NostrEvent, NostrIdentity, CHANNEL, LIFELINE_KIND};
use lifeline_bridge::ws::{self, ClientChannels};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

/// Does `event` satisfy a Nostr REQ `filter` object? We honor `kinds` and any
/// `#<letter>` tag filter (the two Lifeline uses); other keys are ignored.
fn event_matches(ev: &NostrEvent, filter: &Value) -> bool {
    let Some(obj) = filter.as_object() else {
        return false;
    };
    for (key, want) in obj {
        let Some(wanted) = want.as_array() else {
            continue;
        };
        if key == "kinds" {
            if !wanted.iter().any(|v| v.as_u64() == Some(ev.kind as u64)) {
                return false;
            }
        } else if let Some(letter) = key.strip_prefix('#') {
            // Event must carry a tag [letter, value] with value in the wanted set.
            let ok = ev.tags.iter().any(|t| {
                t.first().map(String::as_str) == Some(letter)
                    && t.get(1)
                        .is_some_and(|v| wanted.iter().any(|w| w.as_str() == Some(v.as_str())))
            });
            if !ok {
                return false;
            }
        }
    }
    true
}

/// A single relay connection: reads EVENT/REQ/CLOSE, stores events, honors
/// subscriptions, and forwards newly-published events to matching subscribers on
/// every connection (via a shared broadcast bus).
async fn handle_conn(
    stream: TcpStream,
    store: Arc<Mutex<Vec<NostrEvent>>>,
    bus_tx: broadcast::Sender<NostrEvent>,
) {
    let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
    let (mut write, mut read) = ws.split();
    let mut bus_rx = bus_tx.subscribe();
    // subscription id -> filter list
    let mut subs: HashMap<String, Vec<Value>> = HashMap::new();

    loop {
        tokio::select! {
            incoming = read.next() => {
                let Some(Ok(msg)) = incoming else { break };
                let Message::Text(text) = msg else { continue };
                let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(text.as_str()) else { continue };
                match arr.first().and_then(Value::as_str) {
                    Some("EVENT") => {
                        // ["EVENT", event]
                        let Some(ev) = arr.get(1).and_then(|v| serde_json::from_value::<NostrEvent>(v.clone()).ok()) else { continue };
                        store.lock().unwrap().push(ev.clone());
                        let _ = write.send(Message::text(json!(["OK", ev.id, true, ""]).to_string())).await;
                        let _ = bus_tx.send(ev); // fan out to all connections' subscriptions
                    }
                    Some("REQ") => {
                        // ["REQ", subid, filter, filter, ...]
                        let Some(subid) = arr.get(1).and_then(Value::as_str).map(str::to_string) else { continue };
                        let filters: Vec<Value> = arr[2..].to_vec();
                        // Replay stored matches, then EOSE.
                        let snapshot = store.lock().unwrap().clone();
                        for ev in &snapshot {
                            if filters.iter().any(|f| event_matches(ev, f)) {
                                let _ = write.send(Message::text(json!(["EVENT", subid, ev]).to_string())).await;
                            }
                        }
                        let _ = write.send(Message::text(json!(["EOSE", subid]).to_string())).await;
                        subs.insert(subid, filters);
                    }
                    Some("CLOSE") => {
                        if let Some(subid) = arr.get(1).and_then(Value::as_str) {
                            subs.remove(subid);
                        }
                    }
                    _ => {}
                }
            }
            broadcasted = bus_rx.recv() => {
                let Ok(ev) = broadcasted else { continue };
                for (subid, filters) in &subs {
                    if filters.iter().any(|f| event_matches(&ev, f)) {
                        let _ = write.send(Message::text(json!(["EVENT", subid, ev]).to_string())).await;
                    }
                }
            }
        }
    }
}

/// Spawn the in-process relay; returns the `ws://` URL it listens on.
async fn spawn_relay() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store: Arc<Mutex<Vec<NostrEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let (bus_tx, _) = broadcast::channel::<NostrEvent>(1024);
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(handle_conn(stream, store.clone(), bus_tx.clone()));
        }
    });
    format!("ws://{addr}")
}

/// Recv from a channel with a deadline so a hang fails the test instead of
/// blocking CI forever.
async fn recv_timeout(rx: &mut mpsc::UnboundedReceiver<(u64, Vec<u8>)>) -> Option<(u64, Vec<u8>)> {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .ok()
        .flatten()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_clients_exchange_frames_over_real_websocket() {
    let url = spawn_relay().await;

    // Alice.
    let (alice_out_tx, alice_out_rx) = mpsc::unbounded_channel();
    let (alice_in_tx, mut alice_in_rx) = mpsc::unbounded_channel();
    let alice_peers = Arc::new(Mutex::new(HashMap::new()));
    let alice_url = url.clone();
    let alice_peers_task = alice_peers.clone();
    let alice_task = tokio::spawn(async move {
        let alice_id = NostrIdentity::from_seed(&[1u8; 32]).unwrap();
        let mut ch = ClientChannels {
            outbound: alice_out_rx,
            inbound: alice_in_tx,
            peers: alice_peers_task,
        };
        ws::run(&alice_url, &alice_id, &mut ch).await
    });

    // Bob.
    let (bob_out_tx, bob_out_rx) = mpsc::unbounded_channel();
    let (bob_in_tx, mut bob_in_rx) = mpsc::unbounded_channel();
    let bob_peers = Arc::new(Mutex::new(HashMap::new()));
    let bob_peers_task = bob_peers.clone();
    let bob_task = tokio::spawn(async move {
        let bob_id = NostrIdentity::from_seed(&[2u8; 32]).unwrap();
        let mut ch = ClientChannels {
            outbound: bob_out_rx,
            inbound: bob_in_tx,
            peers: bob_peers_task,
        };
        ws::run(&url, &bob_id, &mut ch).await
    });

    // Let both subscriptions land at the relay before publishing.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Alice broadcasts a discovery beacon; Bob receives it and learns Alice.
    alice_out_tx.send((None, b"alice-beacon".to_vec())).unwrap();
    let (alice_peer, frame) = recv_timeout(&mut bob_in_rx)
        .await
        .expect("bob should get the beacon");
    assert_eq!(frame, b"alice-beacon");
    assert!(bob_peers.lock().unwrap().contains_key(&alice_peer));

    // Bob sends Alice a *directed* frame; the relay routes it back by #p tag.
    bob_out_tx
        .send((Some(alice_peer), b"private-reply".to_vec()))
        .unwrap();
    let (_bob_peer, frame) = recv_timeout(&mut alice_in_rx)
        .await
        .expect("alice should get the reply");
    assert_eq!(frame, b"private-reply");

    // Sanity: Alice never received her own beacon back.
    assert!(
        tokio::time::timeout(Duration::from_millis(300), alice_in_rx.recv())
            .await
            .map(|opt| opt.map(|(_, f)| f))
            .ok()
            .flatten()
            != Some(b"alice-beacon".to_vec())
    );

    // Shut the clients down by dropping their outbound senders.
    drop(alice_out_tx);
    drop(bob_out_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), alice_task).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), bob_task).await;
}

/// The kind/tag constants stay the ones every Lifeline node agrees on.
#[test]
fn frame_kind_and_channel_are_stable() {
    assert_eq!(LIFELINE_KIND, 1998);
    assert_eq!(CHANNEL, "lifeline-mesh");
}
