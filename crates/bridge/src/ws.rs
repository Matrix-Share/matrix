//! A **live WebSocket Nostr relay client** (`ws` feature).
//!
//! [`nostr`](crate::nostr) defines the codec and an in-memory [`MockRelay`] so the
//! whole path is testable without a network. This module is the "for real" half:
//! an async [`tokio-tungstenite`] task that connects to an actual Nostr relay
//! (`wss://relay.example` — or `ws://` for a local one), subscribes to the
//! Lifeline discovery channel plus our own `#p` inbox, publishes outbound frames
//! as signed NIP-01 events, and surfaces inbound frames — reusing the exact same
//! [`NostrEvent`]/[`frame_event`] codec, so a node reaches the global Nostr relay
//! network with **zero engine changes**.
//!
//! It talks to the engine over plain channels (the same shape the engine's
//! `ChannelInterface` already uses): outbound `(Option<PeerId>, frame)` in,
//! inbound `(PeerId, frame)` out, and a shared `PeerId → nostr-pubkey` map that
//! is learned from received events — identical to the in-memory adapter, so the
//! two are drop-in interchangeable.
//!
//! [`tokio-tungstenite`]: https://docs.rs/tokio-tungstenite
//! [`MockRelay`]: crate::nostr::MockRelay
//! [`NostrEvent`]: crate::nostr::NostrEvent

use crate::nostr::{frame_event, NostrEvent, NostrIdentity, CHANNEL, LIFELINE_KIND};
use crate::unhex;
use futures_util::{SinkExt, StreamExt};
use lifeline_proto::codec::b64url_decode;
use lifeline_transport::{peer_id_from_identity, PeerId};
use secp256k1::Secp256k1;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Subscription id we use for our single REQ (relays echo it on every EVENT/EOSE).
const SUBID: &str = "lifeline";

/// Shared `PeerId → nostr-pubkey (hex)` map, learned from received events. The
/// engine side reads it to know which peers are reachable over this bearer; the
/// client writes it as it sees new senders. Same role as `NostrNet::peers`.
pub type PeerMap = Arc<Mutex<HashMap<PeerId, String>>>;

/// Why a single [`run`] connection ended — the caller uses this to decide
/// whether to reconnect (relay dropped us) or stop for good (engine gone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// The relay closed the socket (or a mid-stream read/write failed). The
    /// caller should reconnect.
    RelayClosed,
    /// The engine dropped the outbound sender — the node is shutting this bearer
    /// down. The caller should stop.
    EngineGone,
}

/// The channels a running client speaks to the engine over — mirrors what the
/// engine's `ChannelInterface` produces/consumes.
pub struct ClientChannels {
    /// Frames the engine wants sent: `None` = broadcast to the discovery channel,
    /// `Some(peer)` = directed to that peer's learned Nostr pubkey.
    pub outbound: mpsc::UnboundedReceiver<(Option<PeerId>, Vec<u8>)>,
    /// Frames received from the relay, delivered to the engine.
    pub inbound: mpsc::UnboundedSender<(PeerId, Vec<u8>)>,
    /// Learned peer directory (shared with the engine-facing interface).
    pub peers: PeerMap,
}

/// Errors the client can fail its connection with. Terminal — the caller decides
/// whether to reconnect (a real node loops with backoff).
#[derive(Debug)]
pub enum WsError {
    /// The WebSocket connect/handshake or a send/recv failed.
    Ws(tokio_tungstenite::tungstenite::Error),
}

impl std::fmt::Display for WsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WsError::Ws(e) => write!(f, "nostr websocket error: {e}"),
        }
    }
}
impl std::error::Error for WsError {}

impl From<tokio_tungstenite::tungstenite::Error> for WsError {
    fn from(e: tokio_tungstenite::tungstenite::Error) -> Self {
        WsError::Ws(e)
    }
}

/// The REQ filter set we subscribe with: the shared discovery channel (so we
/// learn peers and receive broadcasts) plus anything `#p`-tagged to us (our
/// directed inbox). Relays hold matching events for offline delivery.
fn subscribe_frame(my_pubkey: &str) -> String {
    json!([
        "REQ",
        SUBID,
        { "kinds": [LIFELINE_KIND], "#L": [CHANNEL] },
        { "kinds": [LIFELINE_KIND], "#p": [my_pubkey] },
    ])
    .to_string()
}

/// Connect to `relay_url`, subscribe, and pump frames between the relay and the
/// engine channels until `channels.outbound` closes or the relay disconnects.
///
/// Runs on the caller's Tokio runtime. `wss://` URLs use TLS (webpki roots);
/// `ws://` is plain (used by the in-process test relay). This is **one
/// connection**: it returns [`Exit`] saying why it ended, and the caller
/// ([`connect_loop`]-style) reconnects on [`Exit::RelayClosed`]. It borrows
/// `channels` so the same outbound receiver survives across reconnects.
///
/// Only a failed initial connect/handshake is an `Err`; a mid-stream drop
/// returns `Ok(Exit::RelayClosed)` so the caller resets its backoff and retries.
pub async fn run(
    relay_url: &str,
    id: &NostrIdentity,
    channels: &mut ClientChannels,
) -> Result<Exit, WsError> {
    let ClientChannels {
        outbound,
        inbound,
        peers,
    } = channels;

    let (ws, _resp) = connect_async(relay_url).await?;
    let (mut write, mut read) = ws.split();

    let my_pk = id.pubkey_hex().to_string();
    if write
        .send(Message::text(subscribe_frame(&my_pk)))
        .await
        .is_err()
    {
        return Ok(Exit::RelayClosed);
    }

    let secp = Secp256k1::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut clock: u64 = 0;

    let exit = loop {
        tokio::select! {
            // Engine wants to send a frame out over Nostr.
            maybe_out = outbound.recv() => {
                let Some((to, frame)) = maybe_out else {
                    break Exit::EngineGone; // engine dropped the sender → stop for good
                };
                let to_pubkey = match to {
                    None => None,
                    Some(peer) => {
                        let pk = peers.lock().unwrap().get(&peer).cloned();
                        match pk {
                            Some(pk) => Some(pk),
                            None => continue, // directed to an unknown peer → drop (as the mock adapter does)
                        }
                    }
                };
                clock += 1;
                let ev = frame_event(id, to_pubkey.as_deref(), &frame, clock);
                let msg = json!(["EVENT", ev]).to_string();
                if write.send(Message::text(msg)).await.is_err() {
                    break Exit::RelayClosed;
                }
            }
            // The relay pushed us a message.
            maybe_msg = read.next() => {
                let msg = match maybe_msg {
                    Some(Ok(m)) => m,
                    _ => break Exit::RelayClosed, // socket closed or read error → reconnect
                };
                match msg {
                    Message::Text(t) => {
                        handle_relay_text(t.as_str(), &my_pk, &secp, &mut seen, peers, inbound);
                    }
                    Message::Ping(_) | Message::Pong(_) => {} // tungstenite auto-pongs
                    Message::Close(_) => break Exit::RelayClosed,
                    _ => {}
                }
            }
        }
    };

    let _ = write.send(Message::Close(None)).await;
    Ok(exit)
}

/// Parse one relay→client message. We only act on `["EVENT", <subid>, <event>]`;
/// `EOSE`/`OK`/`NOTICE`/`CLOSED` are informational and ignored. A received event
/// is verified (Schnorr), de-duplicated, and its frame + sender are surfaced.
fn handle_relay_text(
    text: &str,
    my_pk: &str,
    secp: &Secp256k1<secp256k1::All>,
    seen: &mut HashSet<String>,
    peers: &PeerMap,
    inbound: &mpsc::UnboundedSender<(PeerId, Vec<u8>)>,
) {
    let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(text) else {
        return;
    };
    if arr.first().and_then(Value::as_str) != Some("EVENT") {
        return; // EOSE / OK / NOTICE / CLOSED — nothing to deliver
    }
    // ["EVENT", subscription_id, event]
    let Some(ev_val) = arr.get(2) else { return };
    let Ok(ev) = serde_json::from_value::<NostrEvent>(ev_val.clone()) else {
        return;
    };
    if ev.pubkey == my_pk {
        return; // our own event echoed back
    }
    if !seen.insert(ev.id.clone()) {
        return; // duplicate (relay re-delivery or multi-relay overlap)
    }
    if !ev.verify(secp) {
        return; // forged/tampered — reject, exactly as any Nostr client would
    }
    let (Ok(frame), Some(pk_bytes)) = (b64url_decode(&ev.content), unhex(&ev.pubkey)) else {
        return; // not a Lifeline frame we can decode
    };
    let peer = peer_id_from_identity(&pk_bytes);
    peers.lock().unwrap().insert(peer, ev.pubkey.clone());
    let _ = inbound.send((peer, frame)); // engine gone → drop; the run loop will exit next tick
}
