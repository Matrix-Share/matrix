//! **Internet-over-mesh gateway** — reach the internet *through* a connected node.
//!
//! In a disaster, one node may still have real internet (a satellite uplink, a
//! surviving cell tower, a café Wi-Fi). This module lets **authorized** users
//! reach the internet through that node over the mesh — the mesh becomes the
//! access network, that node the exit. It is a **store-and-forward web proxy**
//! (request/response), which fits a high-latency DTN far better than a live
//! socket tunnel: a client sends a sealed [`NetRequest`] ("fetch this URL"), the
//! gateway authorizes it, fetches on the real internet, and returns a sealed
//! [`NetResponse`].
//!
//! # The security model (the whole point)
//!
//! **Mesh messages are open; internet egress is node-authorized.** Any node
//! relays any bundle (open store-carry-forward), but a gateway performs an actual
//! internet fetch **only** for identities it has granted — everyone else is
//! *refused without a fetch*, while their ordinary mesh messages still relay.
//! This is capability-based access control at the exit, separate from L4 relay.
//!
//! Also enforced here, because an exit that fetches arbitrary URLs is an
//! SSRF/abuse risk:
//! * requests are answered only for **authorized** requesters ([`AccessPolicy`]);
//! * only `http`/`https`, and **never** `localhost` / private / loopback /
//!   link-local targets ([`is_safe_url`]) — the real [`Fetcher`] must additionally
//!   re-check the *resolved* IP to defeat DNS-rebinding;
//! * the gateway operator opts in and scopes who may use it (they bear egress
//!   liability); grants are revocable.
//!
//! Request/response bodies are sealed end-to-end between client and gateway with
//! Lifeline's ordinary E2E path, so relays carrying the bundle never see the URL
//! or the response — only the gateway (the chosen, trusted exit) does, exactly
//! like a VPN/Tor exit.
//!
//! This crate is the transport-agnostic, network-free core (testable without any
//! sockets). The client-side local proxy (an OS HTTP/SOCKS proxy that serialises
//! app requests into sealed `NetRequest` bundles) and the real HTTP [`Fetcher`]
//! are the integration layer.

use lifeline_proto::Address;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::IpAddr;

/// A request to fetch something on the real internet, carried over the mesh to a
/// gateway (sealed E2E — relays never see it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetRequest {
    /// Correlation token echoed in the [`NetResponse`].
    pub id: Vec<u8>,
    /// HTTP method (`GET`, `POST`, …).
    pub method: String,
    /// Absolute URL to fetch.
    pub url: String,
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: Vec<u8>,
}

/// The gateway's reply, sealed back to the requester.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetResponse {
    /// Echoes [`NetRequest::id`].
    pub id: Vec<u8>,
    /// HTTP status, or `0` when the gateway itself refused/failed (see `error`).
    pub status: u16,
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: Vec<u8>,
    /// Set when the gateway refused or the fetch failed (authorization denied,
    /// unsafe URL, network error). `None` on a real HTTP response.
    #[serde(default)]
    pub error: Option<String>,
}

impl NetResponse {
    /// A gateway-level refusal/error (no HTTP status).
    pub fn refused(id: &[u8], reason: impl Into<String>) -> Self {
        NetResponse {
            id: id.to_vec(),
            status: 0,
            headers: Vec::new(),
            body: Vec::new(),
            error: Some(reason.into()),
        }
    }
}

/// Decides which identities a gateway will perform internet fetches for. This is
/// the *internet-egress* gate — it does **not** affect mesh message relay.
pub trait AccessPolicy {
    fn allows(&self, requester: &Address) -> bool;
}

/// The simplest policy: an explicit allow-list the operator grants into.
#[derive(Debug, Clone, Default)]
pub struct AllowList {
    allowed: HashSet<Address>,
}

impl AllowList {
    pub fn new() -> Self {
        Self::default()
    }
    /// Grant a requester internet access through this gateway.
    pub fn grant(&mut self, who: Address) {
        self.allowed.insert(who);
    }
    /// Revoke a previously-granted requester.
    pub fn revoke(&mut self, who: &Address) {
        self.allowed.remove(who);
    }
    pub fn is_granted(&self, who: &Address) -> bool {
        self.allowed.contains(who)
    }
}

impl AccessPolicy for AllowList {
    fn allows(&self, requester: &Address) -> bool {
        self.allowed.contains(requester)
    }
}

/// Performs the actual internet fetch. The real implementation uses an HTTP
/// client (behind a feature); tests use a mock, so the whole gateway is testable
/// without a network.
pub trait Fetcher {
    fn fetch(&self, req: &NetRequest) -> NetResponse;
}

/// The gateway: authorize → SSRF-check → fetch. Refuses the unauthorized (and
/// unsafe URLs) **without performing any fetch**.
pub struct InternetGateway<P: AccessPolicy, F: Fetcher> {
    policy: P,
    fetcher: F,
}

impl<P: AccessPolicy, F: Fetcher> InternetGateway<P, F> {
    pub fn new(policy: P, fetcher: F) -> Self {
        InternetGateway { policy, fetcher }
    }

    /// Handle a `NetRequest` from `requester`. Never fetches unless the requester
    /// is authorized *and* the URL is safe.
    pub fn handle(&self, requester: &Address, req: &NetRequest) -> NetResponse {
        if !self.policy.allows(requester) {
            return NetResponse::refused(&req.id, "internet access not authorized by this gateway");
        }
        if let Err(reason) = is_safe_url(&req.url) {
            return NetResponse::refused(&req.id, reason);
        }
        self.fetcher.fetch(req)
    }

    /// The policy (to grant/revoke on an `AllowList`, etc.).
    pub fn policy_mut(&mut self) -> &mut P {
        &mut self.policy
    }
}

/// SSRF / abuse guard: only allow `http`/`https` to a **public** host. Rejects
/// `localhost`, `*.local`/`*.internal`/`*.localhost`, and IP literals in
/// loopback / private / link-local / unspecified ranges. The real fetcher must
/// *additionally* re-check the resolved IP (a hostname can resolve to a private
/// address — DNS rebinding).
pub fn is_safe_url(raw: &str) -> Result<(), &'static str> {
    let url = url::Url::parse(raw).map_err(|_| "malformed url")?;
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err("only http/https is permitted"),
    }
    let host = url.host().ok_or("url has no host")?;
    match host {
        url::Host::Domain(name) => {
            let n = name.trim_end_matches('.').to_ascii_lowercase();
            if n == "localhost"
                || n.ends_with(".localhost")
                || n.ends_with(".local")
                || n.ends_with(".internal")
                || n.ends_with(".home.arpa")
            {
                return Err("target host is not a public internet host");
            }
            // A bare hostname could still resolve to a private IP; the fetcher
            // must re-check the resolved address (see module docs).
            Ok(())
        }
        url::Host::Ipv4(ip) => reject_private(IpAddr::V4(ip)),
        url::Host::Ipv6(ip) => reject_private(IpAddr::V6(ip)),
    }
}

/// Reject non-public IP literals.
fn reject_private(ip: IpAddr) -> Result<(), &'static str> {
    let bad = match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.octets()[0] == 0
                // carrier-grade NAT 100.64.0.0/10
                || (v4.octets()[0] == 100 && (64..=127).contains(&v4.octets()[1]))
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                // unique-local fc00::/7
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // link-local fe80::/10
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    };
    if bad {
        Err("target IP is not a public internet address")
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(b: u8) -> Address {
        Address::from_hash_bytes([b; 16])
    }

    fn req(url: &str) -> NetRequest {
        NetRequest {
            id: vec![1, 2, 3],
            method: "GET".into(),
            url: url.into(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// A fetcher that records whether it was called (so we can prove the gateway
    /// never fetches for an unauthorized/unsafe request).
    struct MockFetcher {
        calls: std::cell::Cell<u32>,
    }
    impl MockFetcher {
        fn new() -> Self {
            MockFetcher {
                calls: std::cell::Cell::new(0),
            }
        }
    }
    impl Fetcher for MockFetcher {
        fn fetch(&self, req: &NetRequest) -> NetResponse {
            self.calls.set(self.calls.get() + 1);
            NetResponse {
                id: req.id.clone(),
                status: 200,
                headers: vec![("content-type".into(), "text/plain".into())],
                body: b"hello from the internet".to_vec(),
                error: None,
            }
        }
    }

    #[test]
    fn authorized_request_is_fetched() {
        let mut list = AllowList::new();
        list.grant(addr(1));
        let gw = InternetGateway::new(list, MockFetcher::new());
        let resp = gw.handle(&addr(1), &req("https://example.com/status"));
        assert_eq!(resp.status, 200);
        assert!(resp.error.is_none());
        assert_eq!(gw.fetcher.calls.get(), 1);
    }

    #[test]
    fn unauthorized_request_is_refused_without_fetching() {
        let list = AllowList::new(); // nobody granted
        let gw = InternetGateway::new(list, MockFetcher::new());
        let resp = gw.handle(&addr(2), &req("https://example.com"));
        assert_eq!(resp.status, 0);
        assert!(resp.error.as_deref().unwrap().contains("not authorized"));
        assert_eq!(
            gw.fetcher.calls.get(),
            0,
            "must NOT fetch for an unauthorized requester"
        );
    }

    #[test]
    fn revoke_takes_effect() {
        let mut list = AllowList::new();
        list.grant(addr(3));
        let mut gw = InternetGateway::new(list, MockFetcher::new());
        assert_eq!(gw.handle(&addr(3), &req("https://ok.example")).status, 200);
        gw.policy_mut().revoke(&addr(3));
        assert_eq!(gw.handle(&addr(3), &req("https://ok.example")).status, 0);
    }

    #[test]
    fn ssrf_targets_are_refused_even_for_authorized_requesters() {
        let mut list = AllowList::new();
        list.grant(addr(1));
        let gw = InternetGateway::new(list, MockFetcher::new());
        for bad in [
            "http://localhost/admin",
            "http://127.0.0.1:8080/",
            "https://10.0.0.5/",
            "https://192.168.1.1/",
            "https://169.254.169.254/latest/meta-data/", // cloud metadata SSRF classic
            "http://[::1]/",
            "https://router.local/",
            "https://service.internal/",
            "file:///etc/passwd",
            "ftp://example.com/",
        ] {
            let resp = gw.handle(&addr(1), &req(bad));
            assert_eq!(resp.status, 0, "{bad} must be refused");
            assert!(resp.error.is_some(), "{bad} must carry a refusal reason");
        }
        assert_eq!(
            gw.fetcher.calls.get(),
            0,
            "no unsafe URL may reach the fetcher"
        );
    }

    #[test]
    fn public_urls_pass_the_safety_check() {
        assert!(is_safe_url("https://example.com/path?q=1").is_ok());
        assert!(is_safe_url("http://93.184.216.34/").is_ok()); // a public IP literal
        assert!(is_safe_url("https://api.weather.gov/alerts").is_ok());
    }
}
