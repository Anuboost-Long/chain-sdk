//! Files capability — see /agent-docs/capabilities/files/CONTRACT.md.
//! Reading/writing/deleting bytes within the app's own managed directory
//! is std::fs, already fully portable — same reasoning storage.rs
//! established for rusqlite. There is no per-OS branching here.

use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

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

    /// The path to hand a child process as one argv element for a
    /// process-runner `{ fileReference }` argument — see
    /// process-runner/CONTRACT.md. Anything that doesn't resolve to an
    /// existing managed file, malformed references included, is
    /// `NotFound`: to the caller it's all "that file isn't there".
    pub fn process_path(&self, reference: &str) -> Result<String, FilesError> {
        let path = self
            .resolve(reference)
            .map_err(|_| FilesError::NotFound(format!("no such file: {reference}")))?;
        let path = path.to_string_lossy().into_owned();
        #[cfg(windows)]
        let path = extended_length_path(&path);
        Ok(path)
    }
}

/// Windows' `\\?\` form for a path at or over `MAX_PATH` (260), the limit
/// `files` hides real paths for in the first place — see
/// process-runner/research/WINDOWS.md.
#[cfg(any(windows, test))]
fn extended_length_path(path: &str) -> String {
    if path.len() >= 260 && !path.starts_with(r"\\?\") {
        format!(r"\\?\{path}")
    } else {
        path.to_string()
    }
}

/// Errors from `pick()` and `save()`, the two OS-panel calls.
#[derive(Debug)]
pub enum PickError {
    InvalidArgument(String),
    /// A picker is already open in this app.
    Unavailable(String),
    Other(String),
}

pub struct PickOptions {
    pub multiple: bool,
    pub extensions: Vec<String>,
}

pub struct PickedFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// One OS panel (open or save) at a time per app.
static PICKER_OPEN: AtomicBool = AtomicBool::new(false);

struct PickerOpenGuard;

impl Drop for PickerOpenGuard {
    fn drop(&mut self) {
        PICKER_OPEN.store(false, Ordering::SeqCst);
    }
}

fn validate_extensions(extensions: &[String]) -> Result<(), PickError> {
    match extensions.iter().find(|e| !is_valid_extension(e)) {
        Some(bad) => Err(PickError::InvalidArgument(format!(
            "invalid extension {bad:?}: use letters and digits only, with no leading dot"
        ))),
        None => Ok(()),
    }
}

fn open_panel_guard() -> Result<PickerOpenGuard, PickError> {
    if PICKER_OPEN.swap(true, Ordering::SeqCst) {
        return Err(PickError::Unavailable("a file picker or save panel is already open".to_string()));
    }
    Ok(PickerOpenGuard)
}

pub struct SaveOptions {
    pub suggested_name: Option<String>,
    pub extensions: Vec<String>,
}

/// `path` with the first allowed extension appended when it ends in none
/// of them (case-insensitive). No extensions: unchanged.
fn with_allowed_extension(path: PathBuf, extensions: &[String]) -> PathBuf {
    let current = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    match extensions.first() {
        Some(first) if !extensions.iter().any(|e| Some(e.to_ascii_lowercase()) == current) => {
            let mut name = path.clone().into_os_string();
            name.push(format!(".{first}"));
            PathBuf::from(name)
        }
        _ => path,
    }
}

/// Shows the OS save panel attached to `parent` — every call, never a
/// remembered location — and writes `bytes` where the user chose. Returns
/// the chosen file name (never the path), or `None` on cancel. See
/// CONTRACT.md's `save()` section.
pub async fn save<W: HasWindowHandle + HasDisplayHandle + ?Sized>(
    parent: &W,
    bytes: &[u8],
    options: &SaveOptions,
) -> Result<Option<String>, PickError> {
    validate_extensions(&options.extensions)?;
    let _open = open_panel_guard()?;

    let mut dialog = rfd::AsyncFileDialog::new().set_parent(parent).set_can_create_directories(true);
    // Only the last component: a suggested name can't choose the folder.
    if let Some(name) = options.suggested_name.as_deref().and_then(|n| Path::new(n).file_name()) {
        dialog = dialog.set_file_name(name.to_string_lossy());
    }
    if !options.extensions.is_empty() {
        dialog = dialog.add_filter("Allowed files", &options.extensions);
    }
    let Some(handle) = dialog.save_file().await else {
        return Ok(None);
    };

    let path = with_allowed_extension(handle.path().to_path_buf(), &options.extensions);
    fs::write(&path, bytes).map_err(|e| PickError::Other(format!("couldn't save the file: {e}")))?;
    Ok(Some(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()))
}

/// Shows the OS open-file picker attached to `parent` — a sheet on macOS —
/// and reads each chosen file. Empty on cancel. Paths never leave this
/// function; see CONTRACT.md's `pick()` section.
pub async fn pick<W: HasWindowHandle + HasDisplayHandle + ?Sized>(
    parent: &W,
    options: &PickOptions,
) -> Result<Vec<PickedFile>, PickError> {
    validate_extensions(&options.extensions)?;
    let _open = open_panel_guard()?;

    let mut dialog = rfd::AsyncFileDialog::new().set_parent(parent);
    if !options.extensions.is_empty() {
        dialog = dialog.add_filter("Allowed files", &options.extensions);
    }
    let handles = if options.multiple {
        dialog.pick_files().await.unwrap_or_default()
    } else {
        dialog.pick_file().await.into_iter().collect()
    };

    handles
        .iter()
        .map(|handle| {
            let name = handle.file_name();
            let bytes = fs::read(handle.path())
                .map_err(|e| PickError::Other(format!("couldn't read {name}: {e}")))?;
            Ok(PickedFile { name, bytes })
        })
        .collect()
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
    fn process_path_resolves_only_existing_managed_files() {
        let dir = temp_dir();
        let files = Files::open(&dir).unwrap();

        let reference = files.write(b"png bytes", Some("png")).unwrap();
        let path = files.process_path(&reference).unwrap();
        assert!(Path::new(&path).is_absolute() || path.starts_with(r"\\?\"));
        assert!(path.ends_with(&reference));
        assert_eq!(fs::read(&path).unwrap(), b"png bytes");

        files.delete(&reference).unwrap();
        assert!(matches!(files.process_path(&reference), Err(FilesError::NotFound(_))));
        // Malformed references are NotFound here too, not Other.
        assert!(matches!(files.process_path("../escape"), Err(FilesError::NotFound(_))));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn extended_length_prefix_only_at_max_path() {
        let short = r"C:\Users\me\AppData\Roaming\app\files\0123456789abcdef.png";
        assert_eq!(extended_length_path(short), short);

        let long = format!(r"C:\{}\0123456789abcdef.png", "a".repeat(260));
        assert_eq!(extended_length_path(&long), format!(r"\\?\{long}"));
        // Already-prefixed paths are left alone.
        assert_eq!(extended_length_path(&format!(r"\\?\{long}")), format!(r"\\?\{long}"));
    }

    #[test]
    fn save_appends_the_first_extension_only_when_none_matches() {
        let json = vec!["json".to_string(), "txt".to_string()];
        let path = |p: &str| PathBuf::from(p);
        assert_eq!(with_allowed_extension(path("/x/backup"), &json), path("/x/backup.json"));
        assert_eq!(with_allowed_extension(path("/x/backup.JSON"), &json), path("/x/backup.JSON"));
        assert_eq!(with_allowed_extension(path("/x/notes.txt"), &json), path("/x/notes.txt"));
        assert_eq!(with_allowed_extension(path("/x/backup.2026"), &json), path("/x/backup.2026.json"));
        assert_eq!(with_allowed_extension(path("/x/anything"), &[]), path("/x/anything"));
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
