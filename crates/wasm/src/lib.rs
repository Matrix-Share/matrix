//! Browser bindings for Relayne's client-side identity and messaging.
//!
//! Each browser tab holds its OWN keypair — a passphrase-wrapped [`KeyBackup`],
//! the same JSON the node persists — and seals/opens messages with the SAME Rust
//! crypto as the node, compiled to WASM. So a hosted relay can serve many users
//! without ever holding a key or seeing plaintext (option C). The transport only
//! ever moves the opaque bundle bytes `seal` returns.

use lifeline_core::message::{open_bundle, seal_bundle, SealOptions};
use lifeline_core::{Identity, KeyBackup};
use lifeline_proto::{codec, Bundle, IdentityPublic, Payload, PayloadKind};
use wasm_bindgen::prelude::*;

// Errors are plain strings: wasm-bindgen turns them into JS exceptions, and they
// stay callable from native tests (JsError can't be built off-wasm).
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn restore(backup_json: &str, passphrase: &str) -> Result<Identity, String> {
    let backup: KeyBackup = serde_json::from_str(backup_json).map_err(err)?;
    backup.restore(passphrase).map_err(err)
}

/// Mint a new identity; returns its passphrase-wrapped backup as JSON for the
/// browser to store (IndexedDB). `now` is unix seconds from JS — WASM has no clock.
#[wasm_bindgen]
pub fn new_identity(
    now: u64,
    passphrase: &str,
    display_name: Option<String>,
) -> Result<String, String> {
    let mut id = Identity::generate(now);
    if let Some(n) = display_name {
        id.set_display_name(Some(n));
    }
    let backup = KeyBackup::create(&id, passphrase).map_err(err)?;
    serde_json::to_string(&backup).map_err(err)
}

/// This identity's stable address — the routing tag others send to.
#[wasm_bindgen]
pub fn address(backup_json: &str, passphrase: &str) -> Result<String, String> {
    Ok(restore(backup_json, passphrase)?.address().to_string())
}

/// Your public identity record (CBOR). Share it (QR / pairing code) so others can
/// seal messages to you.
#[wasm_bindgen]
pub fn public_record(backup_json: &str, passphrase: &str) -> Result<Vec<u8>, String> {
    codec::to_cbor(&restore(backup_json, passphrase)?.public()).map_err(err)
}

/// The routing address inside a contact's public record (the tag to send to).
#[wasm_bindgen]
pub fn record_address(record: &[u8]) -> Result<String, String> {
    let rec: IdentityPublic = codec::from_cbor(record).map_err(err)?;
    Ok(rec.id.to_string())
}

/// Seal a text message to a recipient's public record. Returns the opaque
/// ciphertext bundle (CBOR) for the transport to carry; the server never sees the text.
#[wasm_bindgen]
pub fn seal(
    backup_json: &str,
    passphrase: &str,
    recipient_record: &[u8],
    now: u64,
    text: &str,
) -> Result<Vec<u8>, String> {
    let me = restore(backup_json, passphrase)?;
    let recipient: IdentityPublic = codec::from_cbor(recipient_record).map_err(err)?;
    let payload = Payload {
        kind: PayloadKind::Text,
        body: Some(text.to_string()),
        coords: None,
        battery_pct: None,
        attach: None,
        group_id: None,
    };
    let bundle = seal_bundle(&me, &recipient, &payload, &SealOptions::normal(now)).map_err(err)?;
    codec::to_cbor(&bundle).map_err(err)
}

/// Open a ciphertext bundle addressed to us; returns the plaintext. Errors if it
/// isn't for us or the sender signature doesn't verify (anti-impersonation).
#[wasm_bindgen]
pub fn open(backup_json: &str, passphrase: &str, bundle_cbor: &[u8]) -> Result<String, String> {
    let me = restore(backup_json, passphrase)?;
    let bundle: Bundle = codec::from_cbor(bundle_cbor).map_err(err)?;
    let opened = open_bundle(&me, &bundle).map_err(err)?;
    Ok(opened.payload.body.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_persists_and_is_passphrase_guarded() {
        let json = new_identity(1000, "pw", Some("Asha".into())).unwrap();
        let addr = address(&json, "pw").unwrap();
        assert!(!addr.is_empty());
        assert_eq!(addr, address(&json, "pw").unwrap()); // stable across restores
        assert!(address(&json, "wrong").is_err()); // wrong passphrase can't open it
        // Two fresh identities differ — each tab is its own user.
        let other = new_identity(1000, "pw", None).unwrap();
        assert_ne!(addr, address(&other, "pw").unwrap());
    }

    #[test]
    fn message_round_trips_end_to_end() {
        let alice = new_identity(1, "a", Some("Alice".into())).unwrap();
        let bob = new_identity(1, "b", Some("Bob".into())).unwrap();
        let bob_rec = public_record(&bob, "b").unwrap();
        assert_eq!(record_address(&bob_rec).unwrap(), address(&bob, "b").unwrap());

        let bundle = seal(&alice, "a", &bob_rec, 1000, "meet at the bridge").unwrap();
        assert_eq!(open(&bob, "b", &bundle).unwrap(), "meet at the bridge");

        // A third party can't open a bundle sealed to Bob.
        let carol = new_identity(1, "c", None).unwrap();
        assert!(open(&carol, "c", &bundle).is_err());
    }
}
