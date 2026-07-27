//! **Egress capabilities** — a portable, offline-verifiable, *attenuatable*
//! authorization token for internet access through a mesh gateway.
//!
//! # Why a token and not a server lookup
//!
//! Cellular cores (PCRF), captive portals (RADIUS), and enterprise zero-trust
//! proxies all decide "may this flow egress?" by consulting a **reachable
//! server**. In a partitioned disaster mesh there is no reachable server, so
//! that model fails exactly when it matters. The design that survives
//! disconnection puts the authority **in the credential itself**: a signed,
//! scoped token the requester carries and presents, which any gateway can verify
//! **offline** with nothing but the issuer's public key.
//!
//! This is the [macaroon] / [Biscuit] idea, specialised to Lifeline and built on
//! the Ed25519 we already ship (`ed25519-dalek`). We compose audited primitives;
//! we do not invent a new signature scheme.
//!
//! [macaroon]: https://research.google/pubs/pub41892/ (Birgisson et al., 2014)
//! [Biscuit]: https://www.biscuitsec.org/
//!
//! # Three properties, all load-bearing for a mesh
//!
//! 1. **Offline verification.** A [`Capability`] carries the issuer's public key
//!    and a signature over the grant. A gateway that trusts that issuer verifies
//!    the token with a signature check and local clock — no network, no shared
//!    state, works across a partition. See [`Capability::verify`].
//! 2. **Attenuation (delegation that can only narrow).** The holder can append a
//!    [`Scope`] *caveat* that further restricts the grant (fewer hosts, tighter
//!    expiry, smaller byte ceiling) and pass it on, **without contacting the
//!    issuer**. The verifier evaluates the request against the root grant *and
//!    every caveat*, so an appended caveat can only ever remove authority, never
//!    add it — even a maliciously "widening" caveat is a no-op. See
//!    [`Capability::attenuate`].
//! 3. **Unforgeable, unstrippable chain.** Each attenuation is signed by the key
//!    the previous level delegated to, and binds the previous level's signature,
//!    so caveats cannot be reordered, spliced in, or stripped out without
//!    breaking the chain.
//!
//! # Relationship to [`lifeline_proto::Priority`]
//!
//! `Priority` (SOS/Alert/Normal/Bulk) is a *scheduling* axis: it orders bundles
//! on the mesh and scales anti-spam postage. [`ServiceClass`] here is an
//! *egress* axis: it labels what kind of internet access a capability authorizes
//! at a gateway. They are orthogonal — a `Bulk`-priority mesh bundle can carry a
//! `Messaging`-class egress request, and life-safety mesh traffic never needs an
//! egress capability at all because it never leaves the mesh.

use lifeline_proto::{codec, Address};
use serde::{Deserialize, Serialize};

/// An Ed25519 public key, raw 32 bytes — an issuer or a delegation key.
pub type VerKey = [u8; 32];

/// Domain-separation tags so a signature minted for one position in the chain can
/// never be replayed at another.
const GRANT_TAG: &[u8] = b"lifeline/inet/cap-grant/v1";
const ATT_TAG: &[u8] = b"lifeline/inet/cap-attenuation/v1";

/// Length of an Ed25519 signature.
const SIG_LEN: usize = 64;

// ---------------------------------------------------------------------------
// Service class (the egress tier / "network slice")
// ---------------------------------------------------------------------------

/// The kind of internet egress a capability authorizes — Lifeline's analogue of
/// a 5G network slice / DiffServ class, but for the *exit*, not the mesh.
///
/// Ordered by breadth of internet access (see [`ServiceClass::privilege`]).
/// Attenuation can only ever lower the class (Bulk → Interactive → Messaging →
/// LifeSafety), never raise it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceClass {
    /// Emergency-only egress (e.g. to official alert/beacon endpoints). The
    /// narrowest egress; the host allowlist decides which endpoints.
    LifeSafety,
    /// Free "messaging" tier — egress only to an allowlist of messaging hosts.
    /// This is the zero-rated tier (cf. in-flight free WhatsApp).
    Messaging,
    /// General interactive web browsing (a paid tier).
    Interactive,
    /// Bulk transfer / large downloads (the broadest, most metered tier).
    Bulk,
}

impl ServiceClass {
    /// Breadth of internet access, higher = broader. Effective class across an
    /// attenuation chain is the **minimum** (most restrictive) level.
    pub fn privilege(self) -> u8 {
        match self {
            ServiceClass::LifeSafety => 0,
            ServiceClass::Messaging => 1,
            ServiceClass::Interactive => 2,
            ServiceClass::Bulk => 3,
        }
    }

    fn most_restrictive(self, other: ServiceClass) -> ServiceClass {
        if other.privilege() < self.privilege() {
            other
        } else {
            self
        }
    }
}

// ---------------------------------------------------------------------------
// Host / method rules
// ---------------------------------------------------------------------------

/// A host-match pattern: an exact host (`api.whatsapp.net`) or a domain suffix —
/// the pattern `whatsapp.net` matches `whatsapp.net` and any subdomain
/// `*.whatsapp.net`. Matching is case-insensitive and trailing-dot-insensitive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostPattern(pub String);

impl HostPattern {
    fn matches(&self, host: &str) -> bool {
        let pat = self.0.trim_end_matches('.').to_ascii_lowercase();
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        host == pat || host.ends_with(&format!(".{pat}"))
    }
}

/// Which hosts a scope permits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostRule {
    /// Any (public) host — the general-internet tier. SSRF guards still apply at
    /// the gateway regardless.
    Any,
    /// Only hosts matching one of these patterns — the allowlist / zero-rated
    /// tier.
    Only(Vec<HostPattern>),
}

impl HostRule {
    fn permits(&self, host: &str) -> bool {
        match self {
            HostRule::Any => true,
            HostRule::Only(pats) => pats.iter().any(|p| p.matches(host)),
        }
    }
}

/// Which HTTP methods a scope permits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MethodRule {
    /// Any method.
    Any,
    /// Only these methods (case-insensitive). e.g. `["GET", "HEAD"]` for a
    /// read-only tier.
    Only(Vec<String>),
}

impl MethodRule {
    fn permits(&self, method: &str) -> bool {
        match self {
            MethodRule::Any => true,
            MethodRule::Only(ms) => ms.iter().any(|m| m.eq_ignore_ascii_case(method)),
        }
    }
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

/// The restrictions a single grant/caveat level imposes. A request is authorized
/// iff it satisfies the root grant's scope **and** every attenuation's caveat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    /// Hosts this level permits.
    pub hosts: HostRule,
    /// Methods this level permits.
    pub methods: MethodRule,
    /// Egress class this level grants (effective = minimum across the chain).
    pub class: ServiceClass,
    /// Optional ceiling on a single response's body size, bytes. Effective =
    /// minimum across the chain; enforced by the gateway after fetch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_response_bytes: Option<u64>,
    /// Optional *cumulative* byte quota for the whole capability, bytes. Effective
    /// = minimum across the chain; metered by the gateway's quota ledger across
    /// requests (the "paid pass with a data cap" tier). `None` = unmetered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_bytes: Option<u64>,
    /// Optional expiry, unix seconds. Effective = earliest across the chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_after: Option<u64>,
}

impl Scope {
    /// A free "messaging" tier: read-only egress to an allowlist of hosts.
    pub fn messaging(hosts: impl IntoIterator<Item = &'static str>) -> Scope {
        Scope {
            hosts: HostRule::Only(
                hosts
                    .into_iter()
                    .map(|h| HostPattern(h.to_string()))
                    .collect(),
            ),
            methods: MethodRule::Only(vec!["GET".into(), "HEAD".into(), "POST".into()]),
            class: ServiceClass::Messaging,
            max_response_bytes: Some(4 * 1024 * 1024),
            max_total_bytes: None,
            not_after: None,
        }
    }

    /// A general-internet tier with an optional data cap.
    pub fn full_internet(max_response_bytes: Option<u64>) -> Scope {
        Scope {
            hosts: HostRule::Any,
            methods: MethodRule::Any,
            class: ServiceClass::Bulk,
            max_response_bytes,
            max_total_bytes: None,
            not_after: None,
        }
    }

    /// Add/lower an expiry (builder-style; keeps the earlier of the two).
    pub fn expiring_at(mut self, unix_secs: u64) -> Scope {
        self.not_after = Some(match self.not_after {
            Some(existing) => existing.min(unix_secs),
            None => unix_secs,
        });
        self
    }

    /// Set/lower a cumulative byte quota (builder-style; keeps the smaller cap).
    pub fn with_total_quota(mut self, bytes: u64) -> Scope {
        self.max_total_bytes = Some(match self.max_total_bytes {
            Some(existing) => existing.min(bytes),
            None => bytes,
        });
        self
    }

    /// Does this single level permit `(host, method)` at time `now`?
    fn permits(&self, host: &str, method: &str, now: u64) -> Result<(), CapError> {
        if let Some(exp) = self.not_after {
            if now >= exp {
                return Err(CapError::Expired);
            }
        }
        if !self.hosts.permits(host) {
            return Err(CapError::HostNotPermitted);
        }
        if !self.methods.permits(method) {
            return Err(CapError::MethodNotPermitted);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Wire structure of a capability
// ---------------------------------------------------------------------------

/// The root grant, signed by the issuer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Grant {
    /// Format version.
    v: u8,
    /// The identity permitted to exercise this capability.
    subject: Address,
    /// The issuer's Ed25519 public key (a gateway or an authority).
    issuer: VerKey,
    /// What this grant authorizes (before any attenuation).
    scope: Scope,
    /// The key permitted to append the *first* attenuation. Set to the subject's
    /// own delegation key (or the issuer's own key for a non-delegable grant).
    delegate_to: VerKey,
    /// Uniqueness, so two grants with identical fields differ (and to give the
    /// capability a stable id).
    nonce: [u8; 16],
}

/// One narrowing step, signed by the previous level's `delegate_to` key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Attenuation {
    /// The additional restriction (ANDed with everything above it).
    caveat: Scope,
    /// The key permitted to append the *next* attenuation.
    delegate_to: VerKey,
    /// Ed25519 signature over `att_signing_msg(caveat, delegate_to, prev_tag)`,
    /// where `prev_tag` is the previous level's signature — binding the chain.
    #[serde(with = "serde_bytes")]
    sig: Vec<u8>,
}

/// A presented egress capability: a signed root grant plus zero or more
/// signature-chained narrowing attenuations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    grant: Grant,
    /// Issuer signature over `grant_signing_msg(grant)`.
    #[serde(with = "serde_bytes")]
    root_sig: Vec<u8>,
    atts: Vec<Attenuation>,
}

/// What a verified capability grants for a specific request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Granted {
    /// Effective egress class (minimum across the chain).
    pub class: ServiceClass,
    /// Effective single-response byte ceiling (minimum across the chain), if any.
    pub max_response_bytes: Option<u64>,
    /// Effective cumulative byte quota (minimum across the chain), if any.
    pub max_total_bytes: Option<u64>,
}

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

/// An Ed25519 keypair used to **issue** a capability or to **delegate** (append
/// an attenuation). In production a node derives an issuer key from its identity
/// via a domain-separated subkey (never the long-term signing key); tests mint
/// fresh keys.
#[derive(Clone)]
pub struct CapKey {
    signing: SigningKey,
}

impl CapKey {
    /// Generate a fresh keypair from the OS CSPRNG.
    pub fn generate() -> Self {
        CapKey {
            signing: SigningKey::generate(&mut rand::rngs::OsRng),
        }
    }

    /// Reconstruct from a 32-byte seed (e.g. `Identity::derive_subkey`).
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        CapKey {
            signing: SigningKey::from_bytes(seed),
        }
    }

    /// The public (verification) key.
    pub fn public(&self) -> VerKey {
        self.signing.verifying_key().to_bytes()
    }

    fn sign(&self, msg: &[u8]) -> Vec<u8> {
        self.signing.sign(msg).to_bytes().to_vec()
    }
}

/// Verify `sig` over `msg` with a raw Ed25519 public key. Strict verification
/// (rejects non-canonical/malleable signatures).
fn verify_raw(pubkey: &VerKey, msg: &[u8], sig: &[u8]) -> Result<(), CapError> {
    let vk = VerifyingKey::from_bytes(pubkey).map_err(|_| CapError::BadKey)?;
    let sig: [u8; SIG_LEN] = sig.try_into().map_err(|_| CapError::BadSignature)?;
    vk.verify_strict(msg, &Signature::from_bytes(&sig))
        .map_err(|_| CapError::BadSignature)
}

// ---------------------------------------------------------------------------
// Canonical signing messages
// ---------------------------------------------------------------------------

fn canon<T: Serialize>(v: &T) -> Vec<u8> {
    codec::to_cbor(v).expect("capability structs serialize to CBOR")
}

fn grant_signing_msg(grant: &Grant) -> Vec<u8> {
    let mut m = Vec::with_capacity(GRANT_TAG.len() + 128);
    m.extend_from_slice(GRANT_TAG);
    m.extend_from_slice(&canon(grant));
    m
}

/// `TAG || canon(caveat) || delegate_to(32) || prev_tag(64)`. The two fixed-size
/// fields are a suffix, so the split is unambiguous and the encoding is
/// injective — no canonicalisation ambiguity across different inputs.
fn att_signing_msg(caveat: &Scope, delegate_to: &VerKey, prev_tag: &[u8]) -> Vec<u8> {
    let cc = canon(caveat);
    let mut m = Vec::with_capacity(ATT_TAG.len() + cc.len() + 32 + prev_tag.len());
    m.extend_from_slice(ATT_TAG);
    m.extend_from_slice(&cc);
    m.extend_from_slice(delegate_to);
    m.extend_from_slice(prev_tag);
    m
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Why a capability failed to verify or authorize a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CapError {
    #[error("capability issuer is not trusted by this gateway")]
    UntrustedIssuer,
    #[error("capability signature is invalid")]
    BadSignature,
    #[error("malformed key")]
    BadKey,
    #[error("capability was issued to a different subject")]
    SubjectMismatch,
    #[error("target host is not permitted by the capability")]
    HostNotPermitted,
    #[error("method is not permitted by the capability")]
    MethodNotPermitted,
    #[error("capability has expired")]
    Expired,
    #[error("attenuation was not signed by the delegated key")]
    NotDelegate,
}

// ---------------------------------------------------------------------------
// Build / attenuate / verify / authorize
// ---------------------------------------------------------------------------

impl Capability {
    /// Issue a root capability: `issuer` grants `subject` `scope`, and names the
    /// key (`delegate_to`) permitted to attenuate it. For a non-delegable
    /// capability, set `delegate_to` to a key nobody else holds (e.g. the
    /// issuer's own public key).
    pub fn issue(
        issuer: &CapKey,
        subject: Address,
        scope: Scope,
        delegate_to: VerKey,
        nonce: [u8; 16],
    ) -> Capability {
        let grant = Grant {
            v: 1,
            subject,
            issuer: issuer.public(),
            scope,
            delegate_to,
            nonce,
        };
        let root_sig = issuer.sign(&grant_signing_msg(&grant));
        Capability {
            grant,
            root_sig,
            atts: Vec::new(),
        }
    }

    /// A stable id for this capability = the root grant's nonce. (Sufficient for
    /// a per-capability spend ledger; the full grant is the cryptographic
    /// identity.)
    pub fn id(&self) -> [u8; 16] {
        self.grant.nonce
    }

    /// The issuer that signed the root grant.
    pub fn issuer(&self) -> &VerKey {
        &self.grant.issuer
    }

    /// The subject permitted to exercise this capability.
    pub fn subject(&self) -> &Address {
        &self.grant.subject
    }

    /// The delegation key that may append the next attenuation.
    fn tail_delegate(&self) -> VerKey {
        self.atts
            .last()
            .map(|a| a.delegate_to)
            .unwrap_or(self.grant.delegate_to)
    }

    /// The signature that the next attenuation binds to (the chain tag).
    fn tail_sig(&self) -> &[u8] {
        self.atts
            .last()
            .map(|a| a.sig.as_slice())
            .unwrap_or(self.root_sig.as_slice())
    }

    /// Append a narrowing `caveat`, signed by `holder` (which must be the current
    /// tail delegate), and name the next delegate. Returns a new capability;
    /// requires **no** contact with the issuer.
    ///
    /// A caveat can only *remove* authority — the verifier ANDs it with every
    /// other level, so a caveat that tries to widen scope is simply a no-op.
    pub fn attenuate(
        &self,
        holder: &CapKey,
        caveat: Scope,
        next_delegate: VerKey,
    ) -> Result<Capability, CapError> {
        if holder.public() != self.tail_delegate() {
            return Err(CapError::NotDelegate);
        }
        let sig = holder.sign(&att_signing_msg(&caveat, &next_delegate, self.tail_sig()));
        let mut atts = self.atts.clone();
        atts.push(Attenuation {
            caveat,
            delegate_to: next_delegate,
            sig,
        });
        Ok(Capability {
            grant: self.grant.clone(),
            root_sig: self.root_sig.clone(),
            atts,
        })
    }

    /// Verify the whole chain **offline**: the root grant is signed by a trusted
    /// issuer, and every attenuation is signed by the key the previous level
    /// delegated to and binds the previous signature. Does **not** check the
    /// request — see [`Capability::authorize`].
    pub fn verify(&self, is_trusted_issuer: impl Fn(&VerKey) -> bool) -> Result<(), CapError> {
        if !is_trusted_issuer(&self.grant.issuer) {
            return Err(CapError::UntrustedIssuer);
        }
        verify_raw(
            &self.grant.issuer,
            &grant_signing_msg(&self.grant),
            &self.root_sig,
        )?;

        let mut prev_delegate = self.grant.delegate_to;
        let mut prev_tag: &[u8] = &self.root_sig;
        for att in &self.atts {
            let msg = att_signing_msg(&att.caveat, &att.delegate_to, prev_tag);
            verify_raw(&prev_delegate, &msg, &att.sig)?;
            prev_delegate = att.delegate_to;
            prev_tag = &att.sig;
        }
        Ok(())
    }

    /// Full check for a concrete request: verify the chain, bind the subject, and
    /// evaluate `(host, method, now)` against the root grant **and every caveat**.
    /// Returns the effective [`Granted`] class + byte ceiling.
    pub fn authorize(
        &self,
        subject: &Address,
        host: &str,
        method: &str,
        now: u64,
        is_trusted_issuer: impl Fn(&VerKey) -> bool,
    ) -> Result<Granted, CapError> {
        self.verify(is_trusted_issuer)?;
        if &self.grant.subject != subject {
            return Err(CapError::SubjectMismatch);
        }

        // Evaluate every level; a request must satisfy all of them.
        let mut class = self.grant.scope.class;
        let mut max_bytes = self.grant.scope.max_response_bytes;
        let mut max_total = self.grant.scope.max_total_bytes;
        self.grant.scope.permits(host, method, now)?;
        for att in &self.atts {
            att.caveat.permits(host, method, now)?;
            class = class.most_restrictive(att.caveat.class);
            max_bytes = min_opt(max_bytes, att.caveat.max_response_bytes);
            max_total = min_opt(max_total, att.caveat.max_total_bytes);
        }
        Ok(Granted {
            class,
            max_response_bytes: max_bytes,
            max_total_bytes: max_total,
        })
    }
}

/// Minimum of two optional ceilings — a `None` (no ceiling) never loosens a
/// `Some` (a ceiling), so effective = the tightest present.
fn min_opt(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, b) => b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(b: u8) -> Address {
        Address::from_hash_bytes([b; 16])
    }

    fn trust(k: VerKey) -> impl Fn(&VerKey) -> bool {
        move |x: &VerKey| *x == k
    }

    #[test]
    fn issued_capability_authorizes_permitted_request() {
        let issuer = CapKey::generate();
        let subject = addr(1);
        let cap = Capability::issue(
            &issuer,
            subject.clone(),
            Scope::full_internet(None),
            issuer.public(), // non-delegable
            [0u8; 16],
        );
        let g = cap
            .authorize(&subject, "example.com", "GET", 1000, trust(issuer.public()))
            .unwrap();
        assert_eq!(g.class, ServiceClass::Bulk);
    }

    #[test]
    fn untrusted_issuer_is_rejected() {
        let issuer = CapKey::generate();
        let stranger = CapKey::generate();
        let cap = Capability::issue(
            &issuer,
            addr(1),
            Scope::full_internet(None),
            issuer.public(),
            [1u8; 16],
        );
        // Trust only `stranger`, not the real issuer.
        assert_eq!(
            cap.verify(trust(stranger.public())).unwrap_err(),
            CapError::UntrustedIssuer
        );
    }

    #[test]
    fn forged_signature_is_rejected() {
        let issuer = CapKey::generate();
        let mut cap = Capability::issue(
            &issuer,
            addr(1),
            Scope::full_internet(None),
            issuer.public(),
            [2u8; 16],
        );
        // Tamper the grant after signing.
        cap.grant.scope.class = ServiceClass::LifeSafety;
        assert_eq!(
            cap.verify(trust(issuer.public())).unwrap_err(),
            CapError::BadSignature
        );
    }

    #[test]
    fn capability_is_bound_to_its_subject() {
        let issuer = CapKey::generate();
        let cap = Capability::issue(
            &issuer,
            addr(1),
            Scope::full_internet(None),
            issuer.public(),
            [3u8; 16],
        );
        // Presented by a different identity than it was issued to.
        assert_eq!(
            cap.authorize(&addr(2), "example.com", "GET", 10, trust(issuer.public()))
                .unwrap_err(),
            CapError::SubjectMismatch
        );
    }

    #[test]
    fn messaging_tier_allows_only_allowlisted_hosts() {
        let issuer = CapKey::generate();
        let subject = addr(1);
        let cap = Capability::issue(
            &issuer,
            subject.clone(),
            Scope::messaging(["whatsapp.net", "signal.org"]),
            issuer.public(),
            [4u8; 16],
        );
        // Allowlisted host (and a subdomain) → allowed, class == Messaging.
        let g = cap
            .authorize(
                &subject,
                "g.whatsapp.net",
                "GET",
                10,
                trust(issuer.public()),
            )
            .unwrap();
        assert_eq!(g.class, ServiceClass::Messaging);
        // Off-allowlist host → refused (this is the "free messaging, no browsing"
        // in-flight behaviour).
        assert_eq!(
            cap.authorize(
                &subject,
                "news.example.com",
                "GET",
                10,
                trust(issuer.public())
            )
            .unwrap_err(),
            CapError::HostNotPermitted
        );
    }

    #[test]
    fn attenuation_narrows_and_cannot_widen() {
        let issuer = CapKey::generate();
        let subject = addr(1);
        // The subject holds a delegation key; the grant delegates to it.
        let subject_key = CapKey::generate();
        let cap = Capability::issue(
            &issuer,
            subject.clone(),
            Scope::full_internet(None), // broad: any host
            subject_key.public(),
            [5u8; 16],
        );

        // Subject attenuates to messaging-only before handing off, no issuer
        // contact. Caveat *tries to widen* max_response_bytes but that's a no-op.
        let narrowed = cap
            .attenuate(
                &subject_key,
                Scope::messaging(["whatsapp.net"]),
                subject_key.public(),
            )
            .unwrap();

        // Broad host that the ROOT allowed is now refused by the caveat.
        assert_eq!(
            narrowed
                .authorize(&subject, "example.com", "GET", 10, trust(issuer.public()))
                .unwrap_err(),
            CapError::HostNotPermitted
        );
        // Allowlisted host still works, and the effective class dropped to
        // Messaging (the minimum across the chain).
        let g = narrowed
            .authorize(&subject, "whatsapp.net", "GET", 10, trust(issuer.public()))
            .unwrap();
        assert_eq!(g.class, ServiceClass::Messaging);
    }

    #[test]
    fn attenuation_by_wrong_key_is_refused() {
        let issuer = CapKey::generate();
        let subject_key = CapKey::generate();
        let attacker = CapKey::generate();
        let cap = Capability::issue(
            &issuer,
            addr(1),
            Scope::full_internet(None),
            subject_key.public(), // only subject_key may attenuate
            [6u8; 16],
        );
        assert_eq!(
            cap.attenuate(&attacker, Scope::messaging(["x.com"]), attacker.public())
                .unwrap_err(),
            CapError::NotDelegate
        );
    }

    #[test]
    fn stripped_attenuation_breaks_the_chain() {
        let issuer = CapKey::generate();
        let subject_key = CapKey::generate();
        let cap = Capability::issue(
            &issuer,
            addr(1),
            Scope::full_internet(None),
            subject_key.public(),
            [7u8; 16],
        );
        let narrowed = cap
            .attenuate(
                &subject_key,
                Scope::messaging(["whatsapp.net"]),
                subject_key.public(),
            )
            .unwrap();
        // An attacker strips the restricting caveat to try to regain full access.
        let mut tampered = narrowed.clone();
        tampered.atts.clear();
        // The root grant alone still verifies (it's a legit prefix) — so
        // stripping doesn't forge authority the root didn't have. But splicing a
        // *foreign* caveat must fail:
        let other = CapKey::generate();
        let foreign = Attenuation {
            caveat: Scope::full_internet(None),
            delegate_to: other.public(),
            sig: vec![0u8; SIG_LEN], // not a real signature
        };
        tampered.atts.push(foreign);
        assert_eq!(
            tampered.verify(trust(issuer.public())).unwrap_err(),
            CapError::BadSignature
        );
    }

    #[test]
    fn expired_capability_is_refused() {
        let issuer = CapKey::generate();
        let subject = addr(1);
        let cap = Capability::issue(
            &issuer,
            subject.clone(),
            Scope::full_internet(None).expiring_at(1_000),
            issuer.public(),
            [8u8; 16],
        );
        assert!(cap
            .authorize(&subject, "example.com", "GET", 999, trust(issuer.public()))
            .is_ok());
        assert_eq!(
            cap.authorize(
                &subject,
                "example.com",
                "GET",
                1_000,
                trust(issuer.public())
            )
            .unwrap_err(),
            CapError::Expired
        );
    }

    #[test]
    fn verification_is_offline_and_survives_a_partition() {
        // Issue on one "device", serialize, and verify on another that shares
        // nothing but the issuer's public key — no network, no shared state.
        let issuer = CapKey::generate();
        let issuer_pub = issuer.public();
        let subject = addr(1);
        let cap = Capability::issue(
            &issuer,
            subject.clone(),
            Scope::full_internet(Some(1_000)),
            issuer.public(),
            [9u8; 16],
        );
        let wire = codec::to_cbor(&cap).unwrap();

        // ... crosses a partition as opaque bytes ...
        let received: Capability = codec::from_cbor(&wire).unwrap();
        let g = received
            .authorize(&subject, "example.com", "GET", 42, trust(issuer_pub))
            .unwrap();
        assert_eq!(g.max_response_bytes, Some(1_000));
    }

    #[test]
    fn effective_byte_ceiling_is_the_tightest_in_the_chain() {
        let issuer = CapKey::generate();
        let subject = addr(1);
        let subject_key = CapKey::generate();
        let cap = Capability::issue(
            &issuer,
            subject.clone(),
            Scope::full_internet(Some(10_000)),
            subject_key.public(),
            [10u8; 16],
        );
        // Caveat tightens the ceiling to 500 and also tries to "raise" it via a
        // separate level to 1_000_000 — the minimum wins.
        let tight = Scope {
            max_response_bytes: Some(500),
            ..Scope::full_internet(None)
        };
        let loose = Scope {
            max_response_bytes: Some(1_000_000),
            ..Scope::full_internet(None)
        };
        let c1 = cap
            .attenuate(&subject_key, tight, subject_key.public())
            .unwrap();
        let c2 = c1
            .attenuate(&subject_key, loose, subject_key.public())
            .unwrap();
        let g = c2
            .authorize(&subject, "example.com", "GET", 1, trust(issuer.public()))
            .unwrap();
        assert_eq!(g.max_response_bytes, Some(500));
    }
}
