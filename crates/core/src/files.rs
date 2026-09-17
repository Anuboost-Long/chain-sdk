//! Files capability — see /agent-docs/capabilities/files/CONTRACT.md.
//! Reading/writing/deleting bytes within the app's own managed directory
//! is std::fs, already fully portable — same reasoning storage.rs
//! established for rusqlite. There is no per-OS branching here.

use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub enum FilesError {
    /// `reference` doesn't correspond to a file that exists (or ever did).
    NotFound(String),
    Other(String),
}

impl FilesError {
    pub fn message(&self) -> &str {
        match self {
            FilesError::NotFound(m) | FilesError::Other(m) => m,
        }
    }
}

impl From<std::io::Error> for FilesError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            FilesError::NotFound(e.to_string())
        } else {
            FilesError::Other(e.to_string())
        }
    }
}

/// One managed directory, created lazily — a sibling of `storage`'s
/// `app.db` inside the same per-app data directory (see CONTRACT.md).
pub struct Files {
    dir: PathBuf,
}

impl Files {
    pub fn open(dir: &Path) -> Result<Self, FilesError> {
        fs::create_dir_all(dir)?;
        Ok(Self { dir: dir.to_path_buf() })
    }

    /// Writes `bytes` under a capability-generated reference — never the
    /// caller's own filename (see CONTRACT.md's Non-goals and the
    /// Windows MAX_PATH note in research/WINDOWS.md). Retries on the
    /// astronomically unlikely event of an id collision.
    pub fn write(&self, bytes: &[u8], extension: Option<&str>) -> Result<String, FilesError> {
        let ext = extension.filter(|e| is_valid_extension(e));
        for _ in 0..5 {
            let id = generate_id();
            let name = match ext {
                Some(ext) => format!("{id}.{ext}"),
                None => id,
            };
            let path = self.dir.join(&name);
            if path.exists() {
                continue;
            }
            fs::write(&path, bytes)?;
            return Ok(name);
        }
        Err(FilesError::Other("could not generate a unique file reference".to_string()))
    }

    pub fn read(&self, reference: &str) -> Result<Vec<u8>, FilesError> {
        Ok(fs::read(self.resolve(reference)?)?)
    }

    /// Idempotent — deleting a reference that's already gone (or never
    /// existed) succeeds, since the caller's intent is already satisfied.
    pub fn delete(&self, reference: &str) -> Result<(), FilesError> {
        if !is_valid_reference(reference) {
            return Err(FilesError::Other(format!("invalid file reference: {reference}")));
        }
        match fs::remove_file(self.dir.join(reference)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// The absolute on-disk path for `reference` — used by
    /// `files_resolve_path` (see templates/lib.rs), which the SDK's
    /// `url()` feeds to Tauri's `convertFileSrc`. Not part of the public
    /// contract itself: the app only ever sees the opaque reference
    /// string, never a real filesystem path.
    pub fn resolve(&self, reference: &str) -> Result<PathBuf, FilesError> {
        if !is_valid_reference(reference) {
            return Err(FilesError::Other(format!("invalid file reference: {reference}")));
        }
        let path = self.dir.join(reference);
        if !path.exists() {
            return Err(FilesError::NotFound(format!("no such file: {reference}")));
        }
        Ok(path)
    }
}

fn is_valid_extension(ext: &str) -> bool {
    !ext.is_empty() && ext.len() <= 16 && ext.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Guards against path traversal — a reference is always exactly the
/// filename `write` generated (an id, optionally `.<extension>`), never a
/// path with separators or `..`.
fn is_valid_reference(reference: &str) -> bool {
    !reference.is_empty()
        && reference.len() <= 128
        && reference.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
        && !reference.starts_with('.')
}

/// 64 bits of OS-seeded randomness (the same source std's HashMap uses to
/// resist collision attacks) hashed together with the current time — good
/// enough uniqueness for a filename without pulling in a `uuid`/`rand`
/// crate just for this.
fn generate_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(nanos);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("chain-files-test-{}", generate_id()))
    }

    #[test]
    fn write_read_delete_round_trip() {
        let dir = temp_dir();
        let files = Files::open(&dir).unwrap();

        let reference = files.write(b"hello world", Some("txt")).unwrap();
        assert!(reference.ends_with(".txt"));

        let bytes = files.read(&reference).unwrap();
        assert_eq!(bytes, b"hello world");

        files.delete(&reference).unwrap();
        assert!(matches!(files.read(&reference), Err(FilesError::NotFound(_))));

        // Idempotent: deleting an already-gone reference is still Ok.
        files.delete(&reference).unwrap();

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_path_traversal_references() {
        let dir = temp_dir();
        let files = Files::open(&dir).unwrap();

        assert!(files.read("../../etc/passwd").is_err());
        assert!(files.read("sub/dir").is_err());
        assert!(files.read(".hidden").is_err());
        assert!(files.delete("../escape").is_err());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_without_extension_has_no_dot() {
        let dir = temp_dir();
        let files = Files::open(&dir).unwrap();

        let reference = files.write(b"data", None).unwrap();
        assert!(!reference.contains('.'));

        fs::remove_dir_all(&dir).ok();
    }
}
