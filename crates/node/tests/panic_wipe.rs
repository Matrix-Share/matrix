//! End-to-end panic-wipe test (G3): persist a node's *real* on-disk artifacts the
//! exact way `main.rs` does — an Argon2id-encrypted `identity.json` and a sealed
//! `state.vault` — then run the wipe and prove both are destroyed while unrelated
//! files survive. This pins `wipe::NODE_ARTIFACTS` to the filenames the node
//! actually writes, so a future rename of either can't silently leave secrets on
//! a "wiped" device.

use lifeline_core::identity::{Identity, KeyBackup};
use lifeline_core::vault::Vault;
use lifeline_core::wipe;
use std::fs;
use std::path::PathBuf;

fn temp_data_dir(tag: &str) -> PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!("lifeline-node-wipe-{}-{}", std::process::id(), tag));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Write the two real secret artifacts exactly as `main.rs::load_or_create_*` do.
fn persist_node_secrets(dir: &std::path::Path, passphrase: &str) {
    // identity.json — Argon2id-encrypted identity secret keys.
    let id = Identity::generate(0);
    let backup = KeyBackup::create(&id, passphrase).unwrap();
    fs::write(
        dir.join("identity.json"),
        serde_json::to_vec_pretty(&backup).unwrap(),
    )
    .unwrap();

    // state.vault — sealed contacts/history/prekeys blob.
    let vault = Vault::create(passphrase).unwrap();
    let blob = vault.seal(b"sensitive: contacts + message history + prekey ring");
    fs::write(dir.join("state.vault"), serde_json::to_vec(&blob).unwrap()).unwrap();
}

#[test]
fn panic_wipe_destroys_the_real_node_artifacts() {
    let dir = temp_data_dir("real");
    persist_node_secrets(&dir, "correct horse battery staple");
    // A file the node did not create (e.g. an operator's note) must survive a
    // targeted wipe.
    fs::write(dir.join("README.txt"), b"not a secret").unwrap();

    assert!(dir.join("identity.json").exists());
    assert!(dir.join("state.vault").exists());

    let report = wipe::wipe_node_data(&dir);

    assert!(
        report.is_complete(),
        "wipe left something behind: {:?}",
        report.failed
    );
    assert_eq!(
        report.erased_count(),
        2,
        "both secret artifacts must be gone"
    );
    assert!(report.bytes_erased > 0);

    // The secrets are gone...
    assert!(!dir.join("identity.json").exists());
    assert!(!dir.join("state.vault").exists());
    // ...and reading them back yields NotFound, not stale ciphertext.
    assert_eq!(
        fs::read(dir.join("state.vault")).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    // ...but the unrelated file is untouched.
    assert_eq!(fs::read(dir.join("README.txt")).unwrap(), b"not a secret");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn panic_wipe_is_safe_on_a_never_provisioned_node() {
    // A node that was opened but never persisted anything: the wipe must succeed
    // with nothing to do (no panic, no error), so the duress action is always safe
    // to invoke.
    let dir = temp_data_dir("empty");
    let report = wipe::wipe_node_data(&dir);
    assert!(report.is_complete());
    assert_eq!(report.erased_count(), 0);
    fs::remove_dir_all(&dir).ok();
}
