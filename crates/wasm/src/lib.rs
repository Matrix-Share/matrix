//! Browser bindings for Relayne's client-side identity.
//!
//! Each browser tab holds its OWN keypair — a passphrase-wrapped [`KeyBackup`],
//! the same JSON the node persists to `identity.json` — and runs the SAME Rust
//! crypto as the node, compiled to WASM. So a hosted relay can serve many users
//! without ever holding a key or seeing plaintext (the honeypot that option "B"
//! would have been). Transport and seal/open are layered on in later slices;
//! this slice is the identity foundation.

use lifeline_core::{Identity, KeyBackup};
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

/// This identity's stable address (share it so others can reach you).
#[wasm_bindgen]
pub fn address(backup_json: &str, passphrase: &str) -> Result<String, String> {
    Ok(restore(backup_json, passphrase)?.address().to_string())
}

/// Your key-exchange public key (32 bytes) — others seal messages to this.
#[wasm_bindgen]
pub fn kex_public(backup_json: &str, passphrase: &str) -> Result<Vec<u8>, String> {
    Ok(restore(backup_json, passphrase)?.kex_public().as_bytes().to_vec())
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
        assert_eq!(kex_public(&json, "pw").unwrap().len(), 32);

        // Two fresh identities differ — each tab is its own user.
        let other = new_identity(1000, "pw", None).unwrap();
        assert_ne!(addr, address(&other, "pw").unwrap());
    }
}
