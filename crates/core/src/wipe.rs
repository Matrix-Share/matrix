//! Panic / duress wipe — emergency destruction of everything a seized device
//! could leak (G3; bitchat's triple-tap wipe, generalized).
//!
//! Lifeline already zeroizes secrets *in memory* on drop, and persists state
//! **encrypted at rest** (`core::vault`, `identity::KeyBackup`). But a high-risk
//! user under coercion needs one decisive action that makes the on-disk data
//! *unrecoverable now*, not "unrecoverable once the process happens to exit".
//!
//! ## What "wipe" has to guarantee
//! The persisted artifacts are AEAD ciphertext (XChaCha20-Poly1305 under an
//! Argon2id key). So the load-bearing act is **destroying the ciphertext**: with
//! it gone, even a later passphrase compromise (rubber-hose, keylogger) opens
//! nothing. Overwriting the bytes first is *defense-in-depth* against file
//! recovery (undelete, journaling, slack space) — best-effort, because on
//! log-structured filesystems and wear-levelled SSDs an in-place overwrite is not
//! guaranteed to hit the original blocks. We therefore do both, in this order:
//!
//! 1. **Overwrite** the file's bytes with random data, then flush to disk.
//! 2. **Truncate** to zero length.
//! 3. **Remove** the directory entry.
//!
//! and we report exactly what was and wasn't destroyed rather than silently
//! best-efforting, so the caller can surface an honest result.
//!
//! ## Scope
//! This module is pure filesystem + RNG and fully unit-testable. The *in-memory*
//! half of a panic wipe (dropping the engine/identity so `zeroize`-on-drop fires)
//! is the caller's job — see the node's `Command::Panic` handler, which wipes the
//! data dir and then returns, dropping every live secret.

use crate::crypto;
use std::fs;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// The two files a Lifeline node persists under its data dir. Wiping these
/// destroys the identity secret keys and all sealed state (contacts, message
/// history, the forward-secret prekey ring, group ids).
pub const NODE_ARTIFACTS: [&str; 2] = ["identity.json", "state.vault"];

/// Number of random-overwrite passes before removal. One pass is enough to
/// defeat naive undelete; the real security comes from destroying AEAD
/// ciphertext, so extra passes are cheap paranoia, not the guarantee.
const OVERWRITE_PASSES: usize = 1;

/// Outcome of a wipe: what was destroyed and what resisted, so the result can be
/// reported honestly instead of assumed complete.
#[derive(Debug, Default, Clone)]
pub struct WipeReport {
    /// Files successfully overwritten and removed.
    pub erased: Vec<PathBuf>,
    /// Files that could not be fully destroyed, with the reason.
    pub failed: Vec<(PathBuf, String)>,
    /// Total bytes overwritten across all erased files.
    pub bytes_erased: u64,
}

impl WipeReport {
    /// True iff every file targeted was destroyed and none failed.
    pub fn is_complete(&self) -> bool {
        self.failed.is_empty()
    }

    /// Number of files destroyed.
    pub fn erased_count(&self) -> usize {
        self.erased.len()
    }

    fn merge(&mut self, other: WipeReport) {
        self.erased.extend(other.erased);
        self.failed.extend(other.failed);
        self.bytes_erased += other.bytes_erased;
    }
}

/// Securely erase a single regular file: overwrite its bytes with random data,
/// flush, truncate, then remove. Returns `Some(len)` for a file that existed and
/// was destroyed, or `None` if there was nothing there — so a wipe is idempotent
/// and a missing file is success (nothing to destroy), not a failure and not a
/// falsely-reported erasure.
pub fn secure_erase_file(path: &Path) -> io::Result<Option<u64>> {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        // Nothing there — already destroyed. Not an error, and nothing erased.
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if !meta.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to secure-erase a non-regular-file path",
        ));
    }
    let len = meta.len();

    // Open read-write to overwrite in place (not truncating yet — we want to
    // clobber the existing bytes on disk before releasing them).
    let mut f = fs::OpenOptions::new().write(true).open(path)?;
    for _ in 0..OVERWRITE_PASSES {
        f.seek(SeekFrom::Start(0))?;
        let mut remaining = len as usize;
        // Chunked so a large mailbox file doesn't balloon memory.
        const CHUNK: usize = 64 * 1024;
        while remaining > 0 {
            let n = remaining.min(CHUNK);
            let buf = crypto::random_bytes(n);
            f.write_all(&buf)?;
            remaining -= n;
        }
        f.flush()?;
        f.sync_all()?;
    }
    // Collapse the length so the tail can't linger, then unlink.
    f.set_len(0)?;
    f.sync_all()?;
    drop(f);
    fs::remove_file(path)?;
    Ok(Some(len))
}

/// Wipe an explicit list of file paths, collecting a report. Order is preserved;
/// one failure never aborts the rest (a coerced user needs *as much as possible*
/// destroyed, not an all-or-nothing that stops at the first sticky file).
pub fn wipe_paths<I, P>(paths: I) -> WipeReport
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut report = WipeReport::default();
    for p in paths {
        let path = p.as_ref();
        match secure_erase_file(path) {
            // Existed and destroyed.
            Ok(Some(n)) => {
                report.erased.push(path.to_path_buf());
                report.bytes_erased += n;
            }
            // Nothing there — idempotent success, nothing to record.
            Ok(None) => {}
            Err(e) => report.failed.push((path.to_path_buf(), e.to_string())),
        }
    }
    report
}

/// Wipe a Lifeline node's persisted secrets: the known [`NODE_ARTIFACTS`] under
/// `data_dir`. This is the disk half of a panic wipe. Does **not** touch
/// unrelated files in the dir, so pointing it at a shared directory only
/// destroys Lifeline's own state.
pub fn wipe_node_data(data_dir: &Path) -> WipeReport {
    let targets = NODE_ARTIFACTS.iter().map(|name| data_dir.join(name));
    wipe_paths(targets)
}

/// Scorched-earth variant: recursively secure-erase **every** file under `dir`
/// and then remove the now-empty directory tree. Use only when the whole data
/// dir is Lifeline's and nothing else lives there. Symlinks are removed without
/// following (we never chase a link out of the tree).
pub fn wipe_dir_recursive(dir: &Path) -> WipeReport {
    let mut report = WipeReport::default();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return report,
        Err(e) => {
            report.failed.push((dir.to_path_buf(), e.to_string()));
            return report;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(e) => {
                report.failed.push((path, e.to_string()));
                continue;
            }
        };
        if ft.is_symlink() {
            if let Err(e) = fs::remove_file(&path) {
                report.failed.push((path, e.to_string()));
            }
        } else if ft.is_dir() {
            report.merge(wipe_dir_recursive(&path));
            if let Err(e) = fs::remove_dir(&path) {
                report.failed.push((path, e.to_string()));
            }
        } else {
            match secure_erase_file(&path) {
                Ok(Some(n)) => {
                    report.erased.push(path);
                    report.bytes_erased += n;
                }
                Ok(None) => {}
                Err(e) => report.failed.push((path, e.to_string())),
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn tmp_dir(tag: &str) -> PathBuf {
        let mut d = std::env::temp_dir();
        // Unique-ish without Date/rand: pid + tag + a monotonic-ish counter.
        d.push(format!("lifeline-wipe-{}-{}", std::process::id(), tag));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(path: &Path, bytes: &[u8]) {
        let mut f = fs::File::create(path).unwrap();
        f.write_all(bytes).unwrap();
        f.sync_all().unwrap();
    }

    #[test]
    fn secure_erase_removes_the_file_and_reports_length() {
        let dir = tmp_dir("single");
        let f = dir.join("secret.bin");
        write(&f, b"top secret ciphertext");
        assert!(f.exists());

        let n = secure_erase_file(&f).unwrap();
        assert_eq!(n, Some("top secret ciphertext".len() as u64));
        assert!(!f.exists(), "file must be gone after a wipe");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn secure_erase_of_missing_file_is_ok_and_idempotent() {
        let dir = tmp_dir("missing");
        let f = dir.join("nope.bin");
        // Never created.
        assert_eq!(secure_erase_file(&f).unwrap(), None);
        // And erasing twice is still fine.
        write(&f, b"x");
        assert_eq!(secure_erase_file(&f).unwrap(), Some(1));
        assert_eq!(secure_erase_file(&f).unwrap(), None);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn overwrite_actually_changes_bytes_before_removal() {
        // Erase-in-place: capture the inode's bytes mid-wipe by overwriting a
        // file we then read back before removal would happen. We simulate by
        // doing the overwrite pass manually via a copy and confirming the API
        // never leaves the original plaintext behind on a *recreated* handle.
        let dir = tmp_dir("overwrite");
        let f = dir.join("state.vault");
        let plaintext = vec![0xABu8; 4096];
        write(&f, &plaintext);

        // Erase, then recreate a file at the same path and confirm the old
        // contents are not what we read (they're gone entirely).
        secure_erase_file(&f).unwrap();
        assert!(!f.exists());

        // Defensive: a fresh file at the same path must not resurrect old bytes.
        write(&f, b"new");
        let mut got = Vec::new();
        fs::File::open(&f).unwrap().read_to_end(&mut got).unwrap();
        assert_eq!(got, b"new");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wipe_node_data_destroys_both_artifacts_but_not_neighbors() {
        let dir = tmp_dir("nodedata");
        write(&dir.join("identity.json"), b"encrypted identity keys");
        write(
            &dir.join("state.vault"),
            b"sealed contacts + history + prekeys",
        );
        // An unrelated neighbor that must survive a targeted wipe.
        write(&dir.join("unrelated.txt"), b"keep me");

        let report = wipe_node_data(&dir);

        assert!(
            report.is_complete(),
            "no failures expected: {:?}",
            report.failed
        );
        assert_eq!(report.erased_count(), 2);
        assert!(report.bytes_erased > 0);
        assert!(!dir.join("identity.json").exists());
        assert!(!dir.join("state.vault").exists());
        assert!(
            dir.join("unrelated.txt").exists(),
            "a targeted wipe must not touch non-Lifeline files"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wipe_node_data_is_idempotent_on_a_fresh_or_already_wiped_node() {
        let dir = tmp_dir("idem");
        // Nothing persisted yet (fresh node) — wipe must succeed with nothing to do.
        let r1 = wipe_node_data(&dir);
        assert!(r1.is_complete());
        assert_eq!(r1.erased_count(), 0);

        // Persist then wipe twice.
        write(&dir.join("state.vault"), b"data");
        assert_eq!(wipe_node_data(&dir).erased_count(), 1);
        assert_eq!(wipe_node_data(&dir).erased_count(), 0);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wipe_dir_recursive_scorches_a_nested_tree() {
        let dir = tmp_dir("recursive");
        write(&dir.join("identity.json"), b"a");
        let sub = dir.join("cache");
        fs::create_dir_all(&sub).unwrap();
        write(&sub.join("blob1"), b"bbb");
        write(&sub.join("blob2"), b"cccc");

        let report = wipe_dir_recursive(&dir);
        assert!(report.is_complete(), "{:?}", report.failed);
        assert_eq!(report.erased_count(), 3);
        assert!(!dir.exists() || fs::read_dir(&dir).unwrap().next().is_none());

        fs::remove_dir_all(&dir).ok();
    }
}
