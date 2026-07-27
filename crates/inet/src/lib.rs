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
//! internet fetch **only** for requests it authorizes — everyone else is
//! *refused without a fetch*, while their ordinary mesh messages still relay.
//! This is capability-based access control at the exit, separate from L4 relay.
//!
//! Authorization is pluggable via [`AccessPolicy`]. Two policies ship here:
//! * [`AllowList`] — the simplest: an operator-maintained set of identities, each
//!   granted full egress. This is the "trivial local issuer" — equivalent to the
//!   gateway having handed every listed identity a full-scope capability.
//! * [`CapabilityPolicy`] — the real model: the requester **presents a
//!   [`capability::Capability`]** (a signed, scoped, *attenuatable*,
//!   offline-verifiable token), and the gateway authorizes strictly to the
//!   capability's scope. See the [`capability`] module for why a portable token —
//!   not a server lookup — is the model that works across a mesh partition.
//!
//! Also enforced here, because an exit that fetches arbitrary URLs is an
//! SSRF/abuse risk:
//! * only `http`/`https`, and **never** `localhost` / private / loopback /
//!   link-local targets ([`is_safe_url`]) — the real [`Fetcher`] must additionally
//!   re-check the *resolved* IP to defeat DNS-rebinding;
//! * a per-request response byte ceiling from the capability's scope.
//!
//! Request/response bodies are sealed end-to-end between client and gateway with
//! Lifeline's ordinary E2E path, so relays carrying the bundle never see the URL
//! or the response — only the gateway (the chosen, trusted exit) does, exactly
//! like a VPN/Tor exit.
//!
//! # No real-time calling — structurally
//!
//! In-flight Wi-Fi blocks VoIP with deep packet inspection. Lifeline does not
//! need to: the transport is store-and-forward request/response, so a persistent
//! low-latency bidirectional media stream has *no representation* here. "Messages
//! flow, calls don't" is a property of the DTN, not a policy we maintain — and
//! [`is_safe_url`] additionally admits only `http`/`https`, never `stun:` /
//! `turn:` / media schemes.
//!
//! This crate is the transport-agnostic, network-free core (testable without any
//! sockets). The client-side local proxy (an OS HTTP/SOCKS proxy that serialises
//! app requests into sealed `NetRequest` bundles) and the real HTTP [`Fetcher`]
//! are the integration layer.

pub mod capability;

pub use capability::{CapError, CapKey, Capability, Scope, ServiceClass};

use lifeline_proto::Address;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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

/// The context an [`AccessPolicy`] decides over: who is asking, what they want to
/// fetch, the capability they presented (if any), and the current time.
pub struct Authz<'a> {
    pub requester: &'a Address,
    pub req: &'a NetRequest,
    pub capability: Option<&'a Capability>,
    /// Current time, unix seconds (for capability expiry).
    pub now: u64,
}

/// Metering handle for an authorized request: which capability to charge and its
/// cumulative quota, so the gateway's [`QuotaLedger`] can enforce a data cap
/// across requests. `None` for the unmetered [`AllowList`] path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Meter {
    /// Capability id (its nonce) — the ledger key.
    pub cap_id: [u8; 16],
    /// Cumulative byte quota for this capability, if any.
    pub max_total_bytes: Option<u64>,
}

/// A policy's verdict on an egress request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Permit the fetch, at this egress class and (optional) response byte
    /// ceiling, optionally metered against a cumulative quota.
    Allow {
        class: ServiceClass,
        max_response_bytes: Option<u64>,
        meter: Option<Meter>,
    },
    /// Refuse; the string is a human-readable reason returned to the requester.
    Deny(String),
}

/// Decides whether — and to what scope — a gateway performs an internet fetch.
/// This is the *internet-egress* gate; it does **not** affect mesh message relay.
///
/// This is the Policy Decision Point (PDP) in NIST-zero-trust terms; the
/// [`InternetGateway`] is the enforcement point (PEP). Keeping them separate lets
/// the decision logic evolve (add reputation/posture inputs, quota ledgers)
/// without touching enforcement.
pub trait AccessPolicy {
    fn decide(&self, authz: &Authz) -> Decision;
}

/// The simplest policy: an explicit allow-list the operator grants into. Every
/// listed identity gets full ([`ServiceClass::Bulk`]) egress; the presented
/// capability, if any, is ignored. This is the "trivial local issuer."
#[derive(Debug, Clone, Default)]
pub struct AllowList {
    allowed: HashSet<Address>,
}

impl AllowList {
    pub fn new() -> Self {
        Self::default()
    }
    /// Grant a requester full internet access through this gateway.
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
    fn decide(&self, authz: &Authz) -> Decision {
        if self.allowed.contains(authz.requester) {
            Decision::Allow {
                class: ServiceClass::Bulk,
                max_response_bytes: None,
                meter: None, // the trivial local issuer is unmetered
            }
        } else {
            Decision::Deny("internet access not authorized by this gateway".into())
        }
    }
}

/// The capability model: the requester must **present** a
/// [`Capability`](capability::Capability) issued (and signed) by an issuer this
/// gateway trusts, and the request is authorized strictly to the capability's
/// (possibly attenuated) scope.
///
/// The gateway holds only a set of **trusted issuer public keys** — no
/// per-identity state to provision or sync. Verification is fully offline, so it
/// works during a partition; attenuation lets a capability be delegated and
/// narrowed hop-by-hop through the mesh with no issuer contact.
#[derive(Debug, Clone, Default)]
pub struct CapabilityPolicy {
    trusted_issuers: HashSet<capability::VerKey>,
    /// Capability ids revoked before their natural expiry. A revoked capability
    /// is refused even if it otherwise verifies and is unexpired — the
    /// issuer/operator's break-glass control (short expiry remains the primary
    /// mechanism; this is for the "revoke *now*" case).
    revoked: HashSet<[u8; 16]>,
}

impl CapabilityPolicy {
    pub fn new() -> Self {
        Self::default()
    }
    /// Trust capabilities signed by this issuer public key.
    pub fn trust(&mut self, issuer: capability::VerKey) {
        self.trusted_issuers.insert(issuer);
    }
    /// Stop trusting an issuer (all capabilities it signed become unusable).
    pub fn untrust(&mut self, issuer: &capability::VerKey) {
        self.trusted_issuers.remove(issuer);
    }
    /// Revoke a specific capability by id (its nonce) before it expires.
    pub fn revoke(&mut self, cap_id: [u8; 16]) {
        self.revoked.insert(cap_id);
    }
    /// Undo a revocation.
    pub fn unrevoke(&mut self, cap_id: &[u8; 16]) {
        self.revoked.remove(cap_id);
    }
    pub fn is_revoked(&self, cap_id: &[u8; 16]) -> bool {
        self.revoked.contains(cap_id)
    }
}

impl AccessPolicy for CapabilityPolicy {
    fn decide(&self, authz: &Authz) -> Decision {
        let Some(cap) = authz.capability else {
            return Decision::Deny("no egress capability presented".into());
        };
        if self.revoked.contains(&cap.id()) {
            return Decision::Deny("capability has been revoked".into());
        }
        let host = match host_of(&authz.req.url) {
            Some(h) => h,
            None => return Decision::Deny("request URL has no host".into()),
        };
        match cap.authorize(authz.requester, &host, &authz.req.method, authz.now, |k| {
            self.trusted_issuers.contains(k)
        }) {
            Ok(g) => Decision::Allow {
                class: g.class,
                max_response_bytes: g.max_response_bytes,
                meter: Some(Meter {
                    cap_id: cap.id(),
                    max_total_bytes: g.max_total_bytes,
                }),
            },
            Err(e) => Decision::Deny(e.to_string()),
        }
    }
}

/// A per-capability cumulative spend ledger — the token-bucket "data cap" that
/// makes a stolen or over-eager capability unable to drain a gateway's scarce
/// backhaul. Keyed by capability id (nonce).
#[derive(Debug, Clone, Default)]
pub struct QuotaLedger {
    spent: HashMap<[u8; 16], u64>,
}

impl QuotaLedger {
    pub fn new() -> Self {
        Self::default()
    }
    /// Bytes already spent by this capability.
    pub fn spent(&self, cap_id: &[u8; 16]) -> u64 {
        self.spent.get(cap_id).copied().unwrap_or(0)
    }
    /// Whether this capability still has quota headroom (true if unmetered).
    pub fn has_headroom(&self, meter: &Meter) -> bool {
        match meter.max_total_bytes {
            Some(cap) => self.spent(&meter.cap_id) < cap,
            None => true,
        }
    }
    /// Record `bytes` spent by a capability (called after a served response).
    pub fn record(&mut self, cap_id: [u8; 16], bytes: u64) {
        let e = self.spent.entry(cap_id).or_insert(0);
        *e = e.saturating_add(bytes);
    }
}

/// Performs the actual internet fetch. The real implementation uses an HTTP
/// client (behind a feature); tests use a mock, so the whole gateway is testable
/// without a network.
pub trait Fetcher {
    fn fetch(&self, req: &NetRequest) -> NetResponse;
}

/// The gateway: authorize → quota-check → SSRF-check → fetch → enforce the
/// per-response byte ceiling → meter the spend. Refuses the unauthorized, the
/// quota-exhausted, and unsafe URLs **without performing any fetch**.
pub struct InternetGateway<P: AccessPolicy, F: Fetcher> {
    policy: P,
    fetcher: F,
    quota: QuotaLedger,
}

impl<P: AccessPolicy, F: Fetcher> InternetGateway<P, F> {
    pub fn new(policy: P, fetcher: F) -> Self {
        InternetGateway {
            policy,
            fetcher,
            quota: QuotaLedger::new(),
        }
    }

    /// Handle a `NetRequest` from `requester`, who may present a `capability`.
    /// `now` is the current unix time (for capability expiry). Never fetches
    /// unless the policy permits it, the capability has quota headroom, *and* the
    /// URL is safe. Metering is `&mut`, so this takes `&mut self`.
    pub fn handle(
        &mut self,
        requester: &Address,
        req: &NetRequest,
        capability: Option<&Capability>,
        now: u64,
    ) -> NetResponse {
        let authz = Authz {
            requester,
            req,
            capability,
            now,
        };
        let (max_bytes, meter) = match self.policy.decide(&authz) {
            Decision::Allow {
                max_response_bytes,
                meter,
                ..
            } => (max_response_bytes, meter),
            Decision::Deny(reason) => return NetResponse::refused(&req.id, reason),
        };
        // Cumulative-quota gate: if the capability is already at/over its data
        // cap, refuse *before* fetching (so an exhausted credential can't keep
        // spending a gateway's backhaul).
        if let Some(m) = &meter {
            if !self.quota.has_headroom(m) {
                return NetResponse::refused(&req.id, "capability data quota exhausted");
            }
        }
        if let Err(reason) = is_safe_url(&req.url) {
            return NetResponse::refused(&req.id, reason);
        }
        let resp = self.fetcher.fetch(req);
        if let Some(cap) = max_bytes {
            if resp.body.len() as u64 > cap {
                return NetResponse::refused(
                    &req.id,
                    "response exceeds the capability's byte ceiling",
                );
            }
        }
        // Meter the served bytes against the capability's cumulative quota.
        if let Some(m) = meter {
            self.quota.record(m.cap_id, resp.body.len() as u64);
        }
        resp
    }

    /// The policy (to grant/revoke on an `AllowList`, trust/revoke on a
    /// `CapabilityPolicy`, etc.).
    pub fn policy_mut(&mut self) -> &mut P {
        &mut self.policy
    }

    /// The quota ledger (to inspect or reset per-capability spend).
    pub fn quota(&self) -> &QuotaLedger {
        &self.quota
    }
}

/// Extract the lowercased host from a URL, or `None` if it has none.
pub fn host_of(raw: &str) -> Option<String> {
    url::Url::parse(raw).ok().and_then(|u| {
        u.host_str()
            .map(|h| h.trim_end_matches('.').to_ascii_lowercase())
    })
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
    /// never fetches for an unauthorized/unsafe request) and can emit a
    /// controllable body size (to exercise the byte ceiling).
    struct MockFetcher {
        calls: std::cell::Cell<u32>,
        body_len: usize,
    }
    impl MockFetcher {
        fn new() -> Self {
            MockFetcher {
                calls: std::cell::Cell::new(0),
                body_len: 23,
            }
        }
        fn with_body_len(body_len: usize) -> Self {
            MockFetcher {
                calls: std::cell::Cell::new(0),
                body_len,
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
                body: vec![b'x'; self.body_len],
                error: None,
            }
        }
    }

    // --- AllowList policy (the trivial local issuer) --------------------------

    #[test]
    fn authorized_request_is_fetched() {
        let mut list = AllowList::new();
        list.grant(addr(1));
        let mut gw = InternetGateway::new(list, MockFetcher::new());
        let resp = gw.handle(&addr(1), &req("https://example.com/status"), None, 0);
        assert_eq!(resp.status, 200);
        assert!(resp.error.is_none());
        assert_eq!(gw.fetcher.calls.get(), 1);
    }

    #[test]
    fn unauthorized_request_is_refused_without_fetching() {
        let list = AllowList::new(); // nobody granted
        let mut gw = InternetGateway::new(list, MockFetcher::new());
        let resp = gw.handle(&addr(2), &req("https://example.com"), None, 0);
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
        assert_eq!(
            gw.handle(&addr(3), &req("https://ok.example"), None, 0)
                .status,
            200
        );
        gw.policy_mut().revoke(&addr(3));
        assert_eq!(
            gw.handle(&addr(3), &req("https://ok.example"), None, 0)
                .status,
            0
        );
    }

    #[test]
    fn ssrf_targets_are_refused_even_for_authorized_requesters() {
        let mut list = AllowList::new();
        list.grant(addr(1));
        let mut gw = InternetGateway::new(list, MockFetcher::new());
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
            let resp = gw.handle(&addr(1), &req(bad), None, 0);
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

    #[test]
    fn voip_style_schemes_have_no_representation() {
        // Real-time calling can't be expressed as a NetRequest, and even the URL
        // guard admits only http/https — never media/signalling schemes.
        for scheme in [
            "stun:stun.l.google.com:19302",
            "turn:turn.example.com",
            "wss://call.example.com",
        ] {
            assert!(is_safe_url(scheme).is_err(), "{scheme} must be refused");
        }
    }

    // --- CapabilityPolicy (the presented-token model) -------------------------

    fn cap_for(issuer: &CapKey, subject: Address, scope: Scope, nonce: u8) -> Capability {
        Capability::issue(issuer, subject, scope, issuer.public(), [nonce; 16])
    }

    #[test]
    fn capability_policy_authorizes_within_scope() {
        let issuer = CapKey::generate();
        let mut policy = CapabilityPolicy::new();
        policy.trust(issuer.public());
        let subject = addr(1);
        let cap = cap_for(
            &issuer,
            subject.clone(),
            Scope::messaging(["whatsapp.net"]),
            1,
        );
        let mut gw = InternetGateway::new(policy, MockFetcher::new());

        // In-scope host → fetched.
        let ok = gw.handle(&subject, &req("https://whatsapp.net/send"), Some(&cap), 100);
        assert_eq!(ok.status, 200);
        // Out-of-scope host → refused without fetching.
        let bad = gw.handle(&subject, &req("https://news.example.com"), Some(&cap), 100);
        assert_eq!(bad.status, 0);
        assert_eq!(
            gw.fetcher.calls.get(),
            1,
            "only the in-scope request fetched"
        );
    }

    #[test]
    fn capability_policy_refuses_missing_or_untrusted_capability() {
        let issuer = CapKey::generate();
        let rogue = CapKey::generate();
        let mut policy = CapabilityPolicy::new();
        policy.trust(issuer.public()); // trust only the real issuer
        let subject = addr(1);
        let mut gw = InternetGateway::new(policy, MockFetcher::new());

        // No capability presented.
        assert_eq!(
            gw.handle(&subject, &req("https://example.com"), None, 0)
                .status,
            0
        );
        // A capability from an untrusted issuer.
        let rogue_cap = cap_for(&rogue, subject.clone(), Scope::full_internet(None), 2);
        assert_eq!(
            gw.handle(&subject, &req("https://example.com"), Some(&rogue_cap), 0)
                .status,
            0
        );
        assert_eq!(gw.fetcher.calls.get(), 0, "neither may reach the fetcher");
    }

    #[test]
    fn capability_byte_ceiling_is_enforced_after_fetch() {
        let issuer = CapKey::generate();
        let mut policy = CapabilityPolicy::new();
        policy.trust(issuer.public());
        let subject = addr(1);
        let cap = cap_for(
            &issuer,
            subject.clone(),
            Scope::full_internet(Some(100)), // 100-byte ceiling
            3,
        );
        // Fetcher returns a 500-byte body — over the ceiling.
        let mut gw = InternetGateway::new(policy, MockFetcher::with_body_len(500));
        let resp = gw.handle(&subject, &req("https://example.com/big"), Some(&cap), 0);
        assert_eq!(resp.status, 0);
        assert!(resp.error.as_deref().unwrap().contains("byte ceiling"));
    }

    #[test]
    fn cumulative_quota_is_metered_and_then_exhausts() {
        let issuer = CapKey::generate();
        let mut policy = CapabilityPolicy::new();
        policy.trust(issuer.public());
        let subject = addr(1);
        // 250-byte cumulative quota; each response is 100 bytes.
        let cap = Capability::issue(
            &issuer,
            subject.clone(),
            Scope::full_internet(None).with_total_quota(250),
            issuer.public(),
            [11u8; 16],
        );
        let mut gw = InternetGateway::new(policy, MockFetcher::with_body_len(100));
        // First two requests fit (100, 200 spent).
        assert_eq!(
            gw.handle(&subject, &req("https://example.com/1"), Some(&cap), 0)
                .status,
            200
        );
        assert_eq!(
            gw.handle(&subject, &req("https://example.com/2"), Some(&cap), 0)
                .status,
            200
        );
        assert_eq!(gw.quota().spent(&cap.id()), 200);
        // Third: still has headroom (200 < 250) so it is served, pushing spend
        // to 300 — the cap bounds total to roughly quota + one response.
        assert_eq!(
            gw.handle(&subject, &req("https://example.com/3"), Some(&cap), 0)
                .status,
            200
        );
        // Fourth: now over quota → refused *without* fetching.
        let calls_before = gw.fetcher.calls.get();
        let resp = gw.handle(&subject, &req("https://example.com/4"), Some(&cap), 0);
        assert_eq!(resp.status, 0);
        assert!(resp.error.as_deref().unwrap().contains("quota exhausted"));
        assert_eq!(
            gw.fetcher.calls.get(),
            calls_before,
            "an exhausted quota must not reach the fetcher"
        );
    }

    #[test]
    fn revoked_capability_is_refused_without_fetching() {
        let issuer = CapKey::generate();
        let mut policy = CapabilityPolicy::new();
        policy.trust(issuer.public());
        let subject = addr(1);
        let cap = cap_for(&issuer, subject.clone(), Scope::full_internet(None), 12);
        let mut gw = InternetGateway::new(policy, MockFetcher::new());
        // Works before revocation.
        assert_eq!(
            gw.handle(&subject, &req("https://example.com"), Some(&cap), 0)
                .status,
            200
        );
        // Operator revokes the capability by id.
        gw.policy_mut().revoke(cap.id());
        let calls_before = gw.fetcher.calls.get();
        let resp = gw.handle(&subject, &req("https://example.com"), Some(&cap), 0);
        assert_eq!(resp.status, 0);
        assert!(resp.error.as_deref().unwrap().contains("revoked"));
        assert_eq!(
            gw.fetcher.calls.get(),
            calls_before,
            "a revoked capability must not reach the fetcher"
        );
    }
}
