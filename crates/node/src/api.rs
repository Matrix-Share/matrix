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
        .route("/api/send_private", post(post_send_private))
        .route("/api/contacts", post(post_contact))
        .route("/api/selftest", get(get_selftest))
        .route("/api/sos", post(post_sos))
        .route("/api/broadcast", post(post_broadcast))
        .route("/api/safe", post(post_safe))
        .route("/api/location", post(post_location))
        .route("/api/qr.svg", get(get_qr))
        .route("/api/ws", get(ws_handler))
        .with_state(state)
}

#[derive(Deserialize)]
struct BroadcastReq {
    body: String,
}

/// Broadcast a message to the whole mesh (every contact).
async fn post_broadcast(
    State(st): State<AppState>,
    Json(req): Json<BroadcastReq>,
) -> impl IntoResponse {
    let _ = st.cmd.send(Command::Broadcast { body: req.body });
    Json(serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct SafeReq {
    note: Option<String>,
}

/// Broadcast "I'm safe" to all contacts (FR-41).
async fn post_safe(State(st): State<AppState>, Json(req): Json<SafeReq>) -> impl IntoResponse {
    let _ = st.cmd.send(Command::Safe { note: req.note });
    Json(serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct LocationReq {
    to: String,
    lat: f64,
    lon: f64,
    acc_m: Option<u32>,
}

/// Share location with a specific contact (FR-43).
async fn post_location(
    State(st): State<AppState>,
    Json(req): Json<LocationReq>,
) -> impl IntoResponse {
    let _ = st.cmd.send(Command::Location {
        to: req.to,
        lat: req.lat,
        lon: req.lon,
        acc_m: req.acc_m,
    });
    Json(serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct SosReq {
    lat: Option<f64>,
    lon: Option<f64>,
    acc_m: Option<u32>,
    battery_pct: Option<u8>,
    note: Option<String>,
}

/// One-tap SOS (FR-40): highest priority, with GPS + battery if the browser
/// granted them.
async fn post_sos(State(st): State<AppState>, Json(req): Json<SosReq>) -> impl IntoResponse {
    let _ = st.cmd.send(Command::Sos {
        lat: req.lat,
        lon: req.lon,
        acc_m: req.acc_m,
        battery_pct: req.battery_pct,
        note: req.note,
    });
    Json(serde_json::json!({ "ok": true }))
}

/// Render this node's invite code as a QR image (FR-3) so another device can
/// scan it in person to add a verified contact (FR-6, TOFU).
async fn get_qr(State(st): State<AppState>) -> impl IntoResponse {
    let code = { st.shared.lock().unwrap().identity.code.clone() };
    let svg = qr_svg(&code).unwrap_or_else(|| {
        "<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'></svg>".into()
    });
    ([(axum::http::header::CONTENT_TYPE, "image/svg+xml")], svg)
}

/// Encode `data` as a QR code and emit a self-contained SVG (one rect per dark
/// module, plus a quiet zone).
fn qr_svg(data: &str) -> Option<String> {
    use qrcode::{Color, QrCode};
    if data.is_empty() {
        return None;
    }
    let code = QrCode::new(data.as_bytes()).ok()?;
    let width = code.width();
    let quiet = 4usize;
    let total = width + quiet * 2;
    let colors = code.to_colors();

    let mut svg = String::with_capacity(total * total * 8);
    svg.push_str(&format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {total} {total}' \
         shape-rendering='crispEdges' width='240' height='240'>\
         <rect width='{total}' height='{total}' fill='#ffffff'/>"
    ));
    for y in 0..width {
        for x in 0..width {
            if colors[y * width + x] == Color::Dark {
                let (px, py) = (x + quiet, y + quiet);
                svg.push_str(&format!(
                    "<rect x='{px}' y='{py}' width='1' height='1' fill='#000000'/>"
                ));
            }
        }
    }
    svg.push_str("</svg>");
    Some(svg)
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
struct SendPrivateReq {
    to: String,
    body: String,
}

/// Send a message via onion routing (FR-49) over auto-selected relays.
async fn post_send_private(
    State(st): State<AppState>,
    Json(req): Json<SendPrivateReq>,
) -> impl IntoResponse {
    let _ = st.cmd.send(Command::SendPrivate {
        to: req.to,
        body: req.body,
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
