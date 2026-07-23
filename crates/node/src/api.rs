//! HTTP + WebSocket API and static GUI serving (axum).

use crate::views::{Command, Snapshot};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

/// Shared state handed to every request handler.
#[derive(Clone)]
pub struct AppState {
    pub cmd: UnboundedSender<Command>,
    pub shared: Arc<Mutex<Snapshot>>,
    pub version: Arc<AtomicU64>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/state", get(get_state))
        .route("/api/send", post(post_send))
        .route("/api/contacts", post(post_contact))
        .route("/api/selftest", get(get_selftest))
        .route("/api/ws", get(ws_handler))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../web/index.html"))
}

async fn get_state(State(st): State<AppState>) -> Json<Snapshot> {
    let snap = st.shared.lock().unwrap().clone();
    Json(snap)
}

#[derive(Deserialize)]
struct SendReq {
    to: String,
    body: String,
    #[serde(default = "default_priority")]
    priority: u8,
}
fn default_priority() -> u8 {
    2
}

async fn post_send(State(st): State<AppState>, Json(req): Json<SendReq>) -> impl IntoResponse {
    let _ = st.cmd.send(Command::Send {
        to: req.to,
        body: req.body,
        priority: req.priority,
    });
    Json(serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct ContactReq {
    code: String,
}

async fn post_contact(
    State(st): State<AppState>,
    Json(req): Json<ContactReq>,
) -> impl IntoResponse {
    let _ = st.cmd.send(Command::AddContact { code: req.code });
    Json(serde_json::json!({ "ok": true }))
}

/// Run the packaged acceptance simulator and return its report — the "testing
/// framework", surfaced in the GUI.
async fn get_selftest() -> impl IntoResponse {
    let res = tokio::task::spawn_blocking(|| {
        use lifeline_sim::scenarios;
        let mut w = scenarios::three_cluster_mule(42);
        let r = w.run(700);
        serde_json::json!({
            "scenario": "3-cluster partition + one moving data mule",
            "sent": r.sent,
            "delivered": r.delivered,
            "verified": r.verified,
            "pct_delivered": r.pct_delivered(),
            "pct_verified": r.pct_verified(),
            "logs_valid": w.all_logs_valid(),
        })
    })
    .await
    .unwrap_or_else(|_| serde_json::json!({ "error": "selftest failed" }));
    Json(res)
}

async fn ws_handler(ws: WebSocketUpgrade, State(st): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| ws_loop(socket, st))
}

/// Push a fresh snapshot to the browser whenever the engine bumps its version.
async fn ws_loop(mut socket: WebSocket, st: AppState) {
    let mut last = u64::MAX;
    let mut ticker = tokio::time::interval(Duration::from_millis(400));
    loop {
        ticker.tick().await;
        let v = st.version.load(Ordering::Relaxed);
        if v == last {
            continue;
        }
        last = v;
        let json = {
            let snap = st.shared.lock().unwrap();
            serde_json::to_string(&*snap)
        };
        let Ok(text) = json else { continue };
        if socket.send(Message::Text(text)).await.is_err() {
            break;
        }
    }
}
