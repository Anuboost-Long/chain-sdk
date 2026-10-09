//! Folders capability — see /agent-docs/capabilities/folders/CONTRACT.md.
//! Real paths inside folders the user granted (picked or dropped), plus
//! the app's declared read-only folders. Every call resolves its path to a
//! canonical one and checks it against the grants first. std::fs and the
//! `notify` crate cover both platforms; only the picker's files-or-folders
//! mode, the Trash and Windows' locked-file retries differ per OS.

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use serde::{Deserialize, Serialize};

use crate::files::{generate_id, open_panel_guard, PickError};

#[derive(Debug)]
pub enum FoldersError {
    InvalidArgument(String),
    /// Outside every grant, or a write under a read-only one.
    NotGranted(String),
    PermissionDenied(String),
    NotFound(String),
    /// Locked by another process.
    Unavailable(String),
    TooLarge(String),
    Unsupported(String),
    Other(String),
}

impl FoldersError {
    pub fn message(&self) -> &str {
        match self {
            FoldersError::InvalidArgument(m)
            | FoldersError::NotGranted(m)
            | FoldersError::PermissionDenied(m)
            | FoldersError::NotFound(m)
            | FoldersError::Unavailable(m)
            | FoldersError::TooLarge(m)
            | FoldersError::Unsupported(m)
            | FoldersError::Other(m) => m,
        }
    }
}

type Result<T> = std::result::Result<T, FoldersError>;

fn io_error(e: std::io::Error, path: &Path) -> FoldersError {
    let message = format!("{}: {e}", path.display());
    match e.kind() {
        ErrorKind::NotFound => FoldersError::NotFound(message),
        ErrorKind::PermissionDenied => FoldersError::PermissionDenied(message),
        ErrorKind::NotADirectory | ErrorKind::IsADirectory | ErrorKind::InvalidFilename => {
            FoldersError::InvalidArgument(message)
        }
        _ if is_locked(&e) => FoldersError::Unavailable(message),
        _ => FoldersError::Other(message),
    }
}

/// ERROR_SHARING_VIOLATION / ERROR_LOCK_VIOLATION: an editor, antivirus or
/// dev server holds the file open.
fn is_locked(e: &std::io::Error) -> bool {
    cfg!(windows) && matches!(e.raw_os_error(), Some(32 | 33))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GrantKind {
    Folder,
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Access {
    ReadWrite,
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GrantSource {
    Picked,
    Dropped,
    Declared,
    /// One of the app's own folders (`AppFolder`).
    App,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppFolder {
    /// Persistent, private to the app.
    Data,
    /// Private to the app; the OS may clear it.
    Temp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub id: String,
    pub path: String,
    pub kind: GrantKind,
    pub access: Access,
    pub source: GrantSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    File,
    Folder,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    pub modified_ms: f64,
}

impl Entry {
    fn new(path: &Path, meta: &fs::Metadata) -> Self {
        let file_type = meta.file_type();
        let kind = if file_type.is_symlink() {
            EntryKind::Symlink
        } else if file_type.is_dir() {
            EntryKind::Folder
        } else if file_type.is_file() {
            EntryKind::File
        } else {
            EntryKind::Other
        };
        Entry {
            path: path.to_string_lossy().into_owned(),
            name: path.file_name().map_or_else(|| path.to_string_lossy(), |n| n.to_string_lossy()).into_owned(),
            kind,
            size: if kind == EntryKind::Folder { 0 } else { meta.len() },
            modified_ms: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0.0, |d| d.as_secs_f64() * 1000.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Created,
    Modified,
    Removed,
    Renamed,
    Other,
    Rescan,
}

#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub path: String,
    pub kind: ChangeKind,
}

struct Declared {
    id: String,
    path: PathBuf,
}

struct Root {
    path: PathBuf,
    kind: GrantKind,
    access: Access,
}

/// The grants for one app: picked and dropped ones persisted in `store`,
/// declared ones fixed for the app's lifetime.
pub struct Folders {
    store: PathBuf,
    declared: Vec<Declared>,
    granted: Mutex<Vec<Grant>>,
    // Canonical; created by with_app_folders.
    app_data: Option<PathBuf>,
    app_temp: Option<PathBuf>,
}

impl Folders {
    /// `declared` entries are absolute or start with `~/`, expanded against
    /// `home`. A corrupt `store` is logged and treated as empty: the user
    /// picks the folders again rather than the app failing to start.
    pub fn open(store: &Path, declared: &[String], home: Option<&Path>) -> Result<Self> {
        let declared = declared
            .iter()
            .map(|entry| {
                let path = match (entry.strip_prefix("~/"), home) {
                    (Some(rest), Some(home)) => home.join(rest),
                    (Some(_), None) => {
                        return Err(FoldersError::InvalidArgument(format!("no home folder to expand {entry}")))
                    }
                    (None, _) => absolute(entry)?,
                };
                Ok(Declared { id: format!("declared:{entry}"), path })
            })
            .collect::<Result<Vec<_>>>()?;

        let granted = match fs::read(store) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                eprintln!("[chain] ignoring unreadable folder grants in {}: {e}", store.display());
                Vec::new()
            }),
            Err(e) if e.kind() == ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(io_error(e, store)),
        };
        Ok(Self { store: store.to_path_buf(), declared, granted: Mutex::new(granted), app_data: None, app_temp: None })
    }

    /// The app's own read-write folders, created if missing. `data` must
    /// not be the folder holding `store` (or the app could edit its own
    /// grants), so it's a folder of its own beside it.
    pub fn with_app_folders(mut self, data: &Path, temp: &Path) -> Result<Self> {
        let create = |path: &Path| {
            fs::create_dir_all(path).map_err(|e| io_error(e, path))?;
            dunce::canonicalize(path).map_err(|e| io_error(e, path))
        };
        let data = create(data)?;
        if self.store.parent().is_some_and(|store_dir| real_path(store_dir).is_ok_and(|dir| dir.starts_with(&data))) {
            return Err(FoldersError::InvalidArgument(format!(
                "the app data folder {} can't contain the grants file",
                data.display()
            )));
        }
        self.app_data = Some(data);
        self.app_temp = Some(create(temp)?);
        Ok(self)
    }

    pub fn app_folder(&self, folder: AppFolder) -> Result<Grant> {
        let (id, path) = match folder {
            AppFolder::Data => ("app:data", &self.app_data),
            AppFolder::Temp => ("app:temp", &self.app_temp),
        };
        let path = path.as_ref().ok_or_else(|| FoldersError::Unsupported("this app has no app folders".to_string()))?;
        // Recreated if something deleted it (the OS clearing temp files).
        fs::create_dir_all(path).map_err(|e| io_error(e, path))?;
        Ok(Grant {
            id: id.to_string(),
            path: path.to_string_lossy().into_owned(),
            kind: GrantKind::Folder,
            access: Access::ReadWrite,
            source: GrantSource::App,
        })
    }

    pub fn grants(&self) -> Vec<Grant> {
        let mut grants = self.granted.lock().expect("folders mutex poisoned").clone();
        grants.extend(self.declared.iter().map(|d| {
            let path = real_path(&d.path).unwrap_or_else(|_| d.path.clone());
            Grant {
                id: d.id.clone(),
                kind: kind_on_disk(&path),
                path: path.to_string_lossy().into_owned(),
                access: Access::ReadOnly,
                source: GrantSource::Declared,
            }
        }));
        grants
    }

    /// Grants an existing file or folder read-write. Granting a path that
    /// already has a grant returns that one.
    pub fn grant(&self, path: &Path, source: GrantSource) -> Result<Grant> {
        let real = dunce::canonicalize(path).map_err(|e| io_error(e, path))?;
        let meta = fs::metadata(&real).map_err(|e| io_error(e, &real))?;
        let real = real.to_string_lossy().into_owned();

        let mut granted = self.granted.lock().expect("folders mutex poisoned");
        if let Some(existing) = granted.iter().find(|g| g.path == real) {
            return Ok(existing.clone());
        }
        let grant = Grant {
            id: generate_id(),
            path: real,
            kind: if meta.is_dir() { GrantKind::Folder } else { GrantKind::File },
            access: Access::ReadWrite,
            source,
        };
        granted.push(grant.clone());
        self.save(&granted)?;
        Ok(grant)
    }

    pub fn revoke(&self, id: &str) -> Result<()> {
        if self.declared.iter().any(|d| d.id == id) {
            return Err(FoldersError::InvalidArgument(format!("{id} is declared by the app and can't be revoked")));
        }
        let mut granted = self.granted.lock().expect("folders mutex poisoned");
        let before = granted.len();
        granted.retain(|g| g.id != id);
        if granted.len() != before {
            self.save(&granted)?;
        }
        Ok(())
    }

    fn save(&self, granted: &[Grant]) -> Result<()> {
        if let Some(parent) = self.store.parent() {
            fs::create_dir_all(parent).map_err(|e| io_error(e, parent))?;
        }
        let json = serde_json::to_vec_pretty(granted).map_err(|e| FoldersError::Other(e.to_string()))?;
        write_atomically(&self.store, &json)
    }

    fn roots(&self) -> Vec<Root> {
        let granted = self.granted.lock().expect("folders mutex poisoned");
        let picked = granted.iter().map(|g| Root { path: PathBuf::from(&g.path), kind: g.kind, access: g.access });
        // Resolved per call, not at startup: a declared folder may appear
        // (an agent's first run) while the app is open.
        let declared = self.declared.iter().filter_map(|d| {
            let path = real_path(&d.path).ok()?;
            Some(Root { kind: kind_on_disk(&path), path, access: Access::ReadOnly })
        });
        let app = [&self.app_data, &self.app_temp]
            .into_iter()
            .flatten()
            .map(|path| Root { path: path.clone(), kind: GrantKind::Folder, access: Access::ReadWrite });
        picked.chain(declared).chain(app).collect()
    }

    /// The canonical path for `path`, once it's known to be inside a grant
    /// giving `access`. `follow` resolves a final symlink to its target;
    /// without it the link itself is the subject (stat, move, delete).
    fn resolve(&self, path: &str, access: Access, follow: bool) -> Result<PathBuf> {
        let path = absolute(path)?;
        let real = match (follow, path.parent(), path.file_name()) {
            (false, Some(parent), Some(name)) => real_path(parent)?.join(name),
            _ => real_path(&path)?,
        };

        let mut read_only = false;
        for root in self.roots() {
            let inside = match root.kind {
                GrantKind::Folder => real.starts_with(&root.path),
                GrantKind::File => real == root.path,
            };
            if !inside {
                continue;
            }
            if access == Access::ReadOnly || root.access == Access::ReadWrite {
                return Ok(real);
            }
            read_only = true;
        }
        Err(FoldersError::NotGranted(if read_only {
            format!("{} is in a read-only folder", real.display())
        } else {
            format!("{} is outside every folder the app was granted", real.display())
        }))
    }

    fn reject_root(&self, real: &Path) -> Result<()> {
        if self.roots().iter().any(|root| root.path == real) {
            return Err(FoldersError::InvalidArgument(format!(
                "{} is a granted folder itself; revoke the grant instead",
                real.display()
            )));
        }
        Ok(())
    }

    /// An existing folder inside any grant, for a process's working directory.
    pub fn working_directory(&self, path: &str) -> Result<PathBuf> {
        let real = self.resolve(path, Access::ReadOnly, true)?;
        let meta = fs::metadata(&real).map_err(|e| io_error(e, &real))?;
        if !meta.is_dir() {
            return Err(FoldersError::InvalidArgument(format!("{} isn't a folder", real.display())));
        }
        Ok(real)
    }

    pub fn list(&self, path: &str, recursive: bool, skip_folders: &[String]) -> Result<Vec<Entry>> {
        let dir = self.resolve(path, Access::ReadOnly, true)?;
        let top = fs::read_dir(&dir).map_err(|e| io_error(e, &dir))?;

        let mut entries = Vec::new();
        let mut pending = vec![top];
        while let Some(reader) = pending.pop() {
            for item in reader.flatten() {
                // DirEntry::metadata doesn't follow symlinks.
                let Ok(meta) = item.metadata() else { continue };
                let entry = Entry::new(&item.path(), &meta);
                if recursive && entry.kind == EntryKind::Folder && !skip_folders.iter().any(|s| *s == entry.name) {
                    if let Ok(sub) = fs::read_dir(item.path()) {
                        pending.push(sub);
                    }
                }
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    pub fn stat(&self, path: &str) -> Result<Entry> {
        let real = self.resolve(path, Access::ReadOnly, false)?;
        let meta = fs::symlink_metadata(&real).map_err(|e| io_error(e, &real))?;
        // resolve() keeps the final name as given; report the one on disk.
        let shown = if meta.is_symlink() { real } else { real_path(&real)? };
        Ok(Entry::new(&shown, &meta))
    }

    pub fn exists(&self, path: &str) -> Result<bool> {
        let real = self.resolve(path, Access::ReadOnly, false)?;
        match fs::symlink_metadata(&real) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
            Err(e) => Err(io_error(e, &real)),
        }
    }

    pub fn read_text(&self, path: &str, max_bytes: Option<u64>) -> Result<String> {
        let bytes = self.read_bytes(path, 0, None, max_bytes)?;
        String::from_utf8(bytes).map_err(|_| FoldersError::InvalidArgument(format!("{path} isn't UTF-8 text")))
    }

    pub fn read_bytes(&self, path: &str, offset: u64, length: Option<u64>, max_bytes: Option<u64>) -> Result<Vec<u8>> {
        let real = self.resolve(path, Access::ReadOnly, true)?;
        let mut file = File::open(&real).map_err(|e| io_error(e, &real))?;
        let meta = file.metadata().map_err(|e| io_error(e, &real))?;
        if meta.is_dir() {
            return Err(FoldersError::InvalidArgument(format!("{} is a folder", real.display())));
        }
        let available = meta.len().saturating_sub(offset);
        let wanted = length.map_or(available, |l| l.min(available));
        if let Some(max) = max_bytes.filter(|max| wanted > *max) {
            return Err(FoldersError::TooLarge(format!("{} is {wanted} bytes, more than {max}", real.display())));
        }
        file.seek(SeekFrom::Start(offset)).map_err(|e| io_error(e, &real))?;
        let mut bytes = Vec::with_capacity(wanted as usize);
        file.take(wanted).read_to_end(&mut bytes).map_err(|e| io_error(e, &real))?;
        Ok(bytes)
    }

    pub fn write(&self, path: &str, bytes: &[u8]) -> Result<()> {
        let real = self.resolve(path, Access::ReadWrite, true)?;
        if real.is_dir() {
            return Err(FoldersError::InvalidArgument(format!("{} is a folder", real.display())));
        }
        write_atomically(&real, bytes)
    }

    pub fn create_folder(&self, path: &str) -> Result<()> {
        let real = self.resolve(path, Access::ReadWrite, true)?;
        match fs::metadata(&real) {
            Ok(meta) if meta.is_dir() => Ok(()),
            Ok(_) => Err(FoldersError::InvalidArgument(format!("{} is a file", real.display()))),
            Err(_) => fs::create_dir_all(&real).map_err(|e| io_error(e, &real)),
        }
    }

    pub fn move_entry(&self, from: &str, to: &str) -> Result<()> {
        let source = self.resolve(from, Access::ReadWrite, false)?;
        let source_meta = fs::symlink_metadata(&source).map_err(|e| io_error(e, &source))?;
        self.reject_root(&source)?;
        let target = self.resolve(to, Access::ReadWrite, false)?;
        if let Ok(target_meta) = fs::symlink_metadata(&target) {
            if !same_entry(&source, &source_meta, &target, &target_meta) {
                return Err(FoldersError::InvalidArgument(format!("{} already exists", target.display())));
            }
        }
        fs::rename(&source, &target).map_err(|e| match e.kind() {
            ErrorKind::CrossesDevices => FoldersError::Other(format!("can't move {} across volumes", source.display())),
            _ => io_error(e, &source),
        })
    }

    pub fn delete(&self, path: &str, to_trash: bool) -> Result<()> {
        let real = self.resolve(path, Access::ReadWrite, false)?;
        let meta = match fs::symlink_metadata(&real) {
            Ok(meta) => meta,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(io_error(e, &real)),
        };
        self.reject_root(&real)?;
        if to_trash {
            return trash(&real);
        }
        let removed = if meta.is_dir() { fs::remove_dir_all(&real) } else { fs::remove_file(&real) };
        match removed {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(io_error(e, &real)),
            _ => Ok(()),
        }
    }

    /// Reports changes under `path` until the returned watch is dropped.
    pub fn watch(
        &self,
        path: &str,
        recursive: bool,
        on_change: impl Fn(Vec<Change>) + Send + 'static,
    ) -> Result<FolderWatch> {
        use notify::{RecursiveMode, Watcher};

        let real = self.resolve(path, Access::ReadOnly, true)?;
        fs::metadata(&real).map_err(|e| io_error(e, &real))?;
        let root = real.to_string_lossy().into_owned();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let changes = match event {
                Ok(event) => to_changes(event, &root),
                Err(_) => vec![Change { path: root.clone(), kind: ChangeKind::Rescan }],
            };
            if !changes.is_empty() {
                on_change(changes);
            }
        })
        .map_err(|e| FoldersError::Other(e.to_string()))?;
        let mode = if recursive { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
        watcher.watch(&real, mode).map_err(|e| FoldersError::Other(e.to_string()))?;
        Ok(FolderWatch { _watcher: watcher })
    }
}

pub struct FolderWatch {
    _watcher: notify::RecommendedWatcher,
}

fn to_changes(event: notify::Event, root: &str) -> Vec<Change> {
    use notify::event::{EventKind, ModifyKind};

    if event.need_rescan() {
        return vec![Change { path: root.to_string(), kind: ChangeKind::Rescan }];
    }
    let kind = match event.kind {
        EventKind::Access(_) => return Vec::new(),
        EventKind::Create(_) => ChangeKind::Created,
        EventKind::Modify(ModifyKind::Name(_)) => ChangeKind::Renamed,
        EventKind::Modify(_) => ChangeKind::Modified,
        EventKind::Remove(_) => ChangeKind::Removed,
        EventKind::Any | EventKind::Other => ChangeKind::Other,
    };
    event.paths.iter().map(|p| Change { path: p.to_string_lossy().into_owned(), kind }).collect()
}

/// A declared entry may be a file (`~/.claude.json`). One that doesn't
/// exist yet can't be told apart, so it counts as a folder until it appears.
fn kind_on_disk(path: &Path) -> GrantKind {
    if fs::metadata(path).is_ok_and(|meta| meta.is_file()) { GrantKind::File } else { GrantKind::Folder }
}

/// Absolute, with no `..` component (a lexical `..` is wrong through symlinks).
fn absolute(path: &str) -> Result<PathBuf> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err(FoldersError::InvalidArgument(format!("{} isn't an absolute path", path.display())));
    }
    if path.components().any(|c| c == Component::ParentDir) {
        return Err(FoldersError::InvalidArgument(format!("{} contains `..`", path.display())));
    }
    Ok(path.to_path_buf())
}

/// The longest existing part of `path` canonicalised (symlinks resolved,
/// the case as stored on disk), with the part that doesn't exist yet
/// appended as given.
fn real_path(path: &Path) -> Result<PathBuf> {
    let mut existing = path;
    let mut missing = Vec::new();
    loop {
        match dunce::canonicalize(existing) {
            Ok(real) => return Ok(missing.iter().rev().fold(real, |acc, name| acc.join(name))),
            Err(e) if e.kind() == ErrorKind::NotFound => match (existing.parent(), existing.file_name()) {
                (Some(parent), Some(name)) => {
                    missing.push(name.to_os_string());
                    existing = parent;
                }
                _ => return Err(io_error(e, path)),
            },
            Err(e) => return Err(io_error(e, path)),
        }
    }
}

/// `Readme.md` → `README.md` on a case-insensitive volume finds the source
/// itself at the target path; that's a rename, not a collision.
fn same_entry(_a: &Path, a: &fs::Metadata, _b: &Path, b: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    #[cfg(not(unix))]
    {
        let _ = (a, b);
        _a.to_string_lossy().to_lowercase() == _b.to_string_lossy().to_lowercase()
    }
}

/// A temp file beside `path`, flushed to disk, then renamed over it: a
/// crash leaves the old content or the new, never half of each.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| FoldersError::InvalidArgument(format!("{} has no folder", path.display())))?;
    if !parent.is_dir() {
        return Err(FoldersError::NotFound(format!("{} doesn't exist", parent.display())));
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = parent.join(format!(".{name}.{}.chain-tmp", generate_id()));

    let written = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        if let Ok(existing) = fs::metadata(path) {
            fs::set_permissions(&temp, existing.permissions())?;
        }
        rename_replacing(&temp, path)
    })();
    written.map_err(|e| {
        let _ = fs::remove_file(&temp);
        io_error(e, path)
    })
}

/// Windows refuses to replace a file another process has open; those
/// holds (antivirus, indexers) are usually brief, so retry a few times.
fn rename_replacing(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut attempt = 0;
    loop {
        match fs::rename(from, to) {
            Err(e) if is_locked(&e) && attempt < 5 => {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(50 * attempt));
            }
            result => return result,
        }
    }
}

#[cfg(target_os = "macos")]
fn trash(path: &Path) -> Result<()> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| FoldersError::Other(format!("couldn't move {} to the Trash: {}", path.display(), e.localizedDescription())))
}

#[cfg(not(target_os = "macos"))]
fn trash(path: &Path) -> Result<()> {
    Err(FoldersError::Unsupported(format!("moving {} to the Trash isn't supported on this platform yet", path.display())))
}

pub struct PickOptions {
    pub multiple: bool,
    /// Files as well as folders.
    pub files: bool,
}

/// Shows the OS folder picker attached to `parent` (a sheet on macOS) and
/// returns what the user chose — empty on cancel. Granting is the caller's
/// next step, with `Folders::grant`.
pub async fn pick<W: HasWindowHandle + HasDisplayHandle + ?Sized>(
    parent: &W,
    options: &PickOptions,
) -> Result<Vec<PathBuf>> {
    let _open = open_panel_guard().map_err(|e| match e {
        PickError::Unavailable(m) => FoldersError::Unavailable(m),
        PickError::InvalidArgument(m) => FoldersError::InvalidArgument(m),
        PickError::Other(m) => FoldersError::Other(m),
    })?;
    let dialog = rfd::AsyncFileDialog::new().set_parent(parent).set_can_create_directories(true);
    let handles = match (options.files, options.multiple) {
        #[cfg(target_os = "macos")]
        (true, true) => dialog.pick_files_or_folders().await.unwrap_or_default(),
        #[cfg(target_os = "macos")]
        (true, false) => dialog.pick_file_or_folder().await.into_iter().collect(),
        #[cfg(not(target_os = "macos"))]
        (true, _) => {
            let _ = dialog;
            return Err(FoldersError::Unsupported(
                "choosing files and folders in one picker isn't supported on this platform".to_string(),
            ));
        }
        (false, true) => dialog.pick_folders().await.unwrap_or_default(),
        (false, false) => dialog.pick_folder().await.into_iter().collect(),
    };
    Ok(handles.iter().map(|h| h.path().to_path_buf()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    struct Fixture {
        base: PathBuf,
        project: PathBuf,
        folders: Folders,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    /// `<tmp>/base/{project, outside, declared}`, with `project` granted and
    /// `declared` read-only. Paths are canonical (`/var` → `/private/var`).
    fn fixture() -> Fixture {
        let base = std::env::temp_dir().join(format!("chain-folders-test-{}", generate_id()));
        fs::create_dir_all(base.join("project/src")).unwrap();
        fs::create_dir_all(base.join("outside")).unwrap();
        fs::create_dir_all(base.join("declared")).unwrap();
        let base = dunce::canonicalize(&base).unwrap();
        let declared = vec![base.join("declared").to_string_lossy().into_owned()];
        let folders = Folders::open(&base.join("grants.json"), &declared, None).unwrap();
        folders.grant(&base.join("project"), GrantSource::Picked).unwrap();
        Fixture { project: base.join("project"), base, folders }
    }

    fn s(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn grants_survive_reopening_and_dedupe() {
        let f = fixture();
        let again = f.folders.grant(&f.project, GrantSource::Picked).unwrap();
        let reopened = Folders::open(&f.base.join("grants.json"), &[], None).unwrap();
        let grants = reopened.grants();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].id, again.id);
        assert_eq!(grants[0].path, s(&f.project));

        reopened.revoke(&again.id).unwrap();
        reopened.revoke(&again.id).unwrap();
        assert!(Folders::open(&f.base.join("grants.json"), &[], None).unwrap().grants().is_empty());
    }

    #[test]
    fn rejects_paths_outside_grants_relative_paths_and_dotdot() {
        let f = fixture();
        fs::write(f.base.join("outside/secret"), "x").unwrap();
        assert!(matches!(f.folders.read_text(&s(&f.base.join("outside/secret")), None), Err(FoldersError::NotGranted(_))));
        assert!(matches!(f.folders.exists(&s(&f.base.join("outside/secret"))), Err(FoldersError::NotGranted(_))));
        assert!(matches!(f.folders.read_text("project/a", None), Err(FoldersError::InvalidArgument(_))));
        let dotdot = format!("{}/../outside/secret", s(&f.project));
        assert!(matches!(f.folders.read_text(&dotdot, None), Err(FoldersError::InvalidArgument(_))));
    }

    #[test]
    fn follows_symlinks_only_into_grants() {
        let f = fixture();
        fs::write(f.base.join("outside/secret"), "secret").unwrap();
        fs::write(f.project.join("src/real.txt"), "inside").unwrap();
        std::os::unix::fs::symlink(f.base.join("outside/secret"), f.project.join("escape")).unwrap();
        std::os::unix::fs::symlink(f.project.join("src/real.txt"), f.project.join("alias")).unwrap();

        assert!(matches!(f.folders.read_text(&s(&f.project.join("escape")), None), Err(FoldersError::NotGranted(_))));
        assert_eq!(f.folders.read_text(&s(&f.project.join("alias")), None).unwrap(), "inside");
        // The link itself lives inside the grant, so it can be inspected and removed.
        assert_eq!(f.folders.stat(&s(&f.project.join("escape"))).unwrap().kind, EntryKind::Symlink);
        f.folders.delete(&s(&f.project.join("escape")), false).unwrap();
        assert_eq!(fs::read_to_string(f.base.join("outside/secret")).unwrap(), "secret");
    }

    #[test]
    fn case_and_var_symlink_differences_still_match_the_grant() {
        let f = fixture();
        fs::write(f.project.join("src/Main.rs"), "fn main() {}").unwrap();
        let through_tmp = std::env::temp_dir().join(f.base.file_name().unwrap()).join("project/SRC/main.rs");
        assert_eq!(f.folders.read_text(&s(&through_tmp), None).unwrap(), "fn main() {}");
        assert_eq!(f.folders.stat(&s(&through_tmp)).unwrap().path, s(&f.project.join("src/Main.rs")));
    }

    #[test]
    fn declared_folders_are_read_only() {
        let f = fixture();
        let declared = f.base.join("declared");
        fs::write(declared.join("log.jsonl"), "0123456789").unwrap();
        assert_eq!(f.folders.read_bytes(&s(&declared.join("log.jsonl")), 4, Some(3), None).unwrap(), b"456");
        assert!(f.folders.read_bytes(&s(&declared.join("log.jsonl")), 20, None, None).unwrap().is_empty());
        assert!(matches!(f.folders.write(&s(&declared.join("new")), b"x"), Err(FoldersError::NotGranted(_))));
        let id = f.folders.grants().into_iter().find(|g| g.source == GrantSource::Declared).unwrap().id;
        assert!(matches!(f.folders.revoke(&id), Err(FoldersError::InvalidArgument(_))));
    }

    #[test]
    fn a_declared_file_grants_that_file_alone_read_only() {
        let base = dunce::canonicalize(std::env::temp_dir()).unwrap().join(format!("chain-folders-file-{}", generate_id()));
        fs::create_dir_all(base.join(".codex")).unwrap();
        fs::write(base.join(".codex/config.toml"), "secret = 1").unwrap();
        let auth = base.join(".codex/auth.json");
        let later = base.join(".claude.json");
        let folders = Folders::open(&base.join("grants.json"), &[s(&auth), s(&later)], None).unwrap();

        // Not there yet: NOT_FOUND, like a missing declared folder.
        assert!(matches!(folders.read_text(&s(&auth), None), Err(FoldersError::NotFound(_))));

        fs::write(&auth, r#"{"token":"t"}"#).unwrap();
        assert_eq!(folders.read_text(&s(&auth), None).unwrap(), r#"{"token":"t"}"#);
        let grant = folders.grants().into_iter().find(|g| g.path == s(&auth)).unwrap();
        assert_eq!((grant.kind, grant.access), (GrantKind::File, Access::ReadOnly));
        // Nothing beside it, and nothing written to it.
        assert!(matches!(folders.read_text(&s(&base.join(".codex/config.toml")), None), Err(FoldersError::NotGranted(_))));
        assert!(matches!(folders.list(&s(&base.join(".codex")), false, &[]), Err(FoldersError::NotGranted(_))));
        assert!(matches!(folders.write(&s(&auth), b"{}"), Err(FoldersError::NotGranted(_))));
        assert!(matches!(folders.list(&s(&auth), false, &[]), Err(FoldersError::InvalidArgument(_))));

        let missing = folders.grants().into_iter().find(|g| g.path == s(&later)).unwrap();
        assert_eq!(missing.kind, GrantKind::Folder, "unknowable until it exists");
        fs::write(&later, "{}").unwrap();
        assert_eq!(folders.grants().into_iter().find(|g| g.path == s(&later)).unwrap().kind, GrantKind::File);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_missing_declared_folder_is_not_found_not_ungranted() {
        let base = dunce::canonicalize(std::env::temp_dir()).unwrap().join(format!("chain-folders-missing-{}", generate_id()));
        let folders = Folders::open(&base.join("grants.json"), &[s(&base.join("later"))], None).unwrap();
        assert!(matches!(folders.list(&s(&base.join("later")), false, &[]), Err(FoldersError::NotFound(_))));
        assert!(matches!(folders.watch(&s(&base.join("later")), true, |_| {}), Err(FoldersError::NotFound(_))));
    }

    #[test]
    fn writes_atomically_keeping_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let f = fixture();
        let env = f.project.join(".env");
        fs::write(&env, "A=1\n").unwrap();
        fs::set_permissions(&env, fs::Permissions::from_mode(0o600)).unwrap();

        f.folders.write(&s(&env), b"# kept\nA=2\n").unwrap();
        assert_eq!(fs::read_to_string(&env).unwrap(), "# kept\nA=2\n");
        assert_eq!(fs::metadata(&env).unwrap().permissions().mode() & 0o777, 0o600);
        let leftovers: Vec<_> = fs::read_dir(&f.project).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".chain-tmp")).collect();
        assert!(leftovers.is_empty());
        assert!(matches!(f.folders.write(&s(&f.project.join("no/such/file")), b"x"), Err(FoldersError::NotFound(_))));
    }

    #[test]
    fn reads_text_strictly_and_caps_bytes() {
        let f = fixture();
        fs::write(f.project.join("bin"), [0xff, 0xfe, 0x00]).unwrap();
        assert!(matches!(f.folders.read_text(&s(&f.project.join("bin")), None), Err(FoldersError::InvalidArgument(_))));
        assert!(matches!(f.folders.read_bytes(&s(&f.project.join("bin")), 0, None, Some(2)), Err(FoldersError::TooLarge(_))));
        assert!(matches!(f.folders.read_text(&s(&f.project), None), Err(FoldersError::InvalidArgument(_))));
    }

    #[test]
    fn lists_recursively_skipping_named_folders_without_entering_symlinks() {
        let f = fixture();
        fs::create_dir_all(f.project.join("node_modules/pkg")).unwrap();
        fs::write(f.project.join("node_modules/pkg/index.js"), "").unwrap();
        fs::write(f.project.join("src/a.ts"), "abc").unwrap();
        std::os::unix::fs::symlink(&f.project, f.project.join("src/loop")).unwrap();

        let entries = f.folders.list(&s(&f.project), true, &["node_modules".to_string()]).unwrap();
        let paths: Vec<_> = entries.iter().map(|e| e.path.strip_prefix(&s(&f.project)).unwrap().to_string()).collect();
        assert!(paths.contains(&"/node_modules".to_string()));
        assert!(!paths.iter().any(|p| p.starts_with("/node_modules/")));
        let a = entries.iter().find(|e| e.name == "a.ts").unwrap();
        assert_eq!((a.kind, a.size), (EntryKind::File, 3));
        assert_eq!(entries.iter().find(|e| e.name == "loop").unwrap().kind, EntryKind::Symlink);
        assert!(!paths.iter().any(|p| p.starts_with("/src/loop/")));
        let src = paths.iter().position(|p| p == "/src").unwrap();
        assert!(src < paths.iter().position(|p| p == "/src/a.ts").unwrap());
    }

    #[test]
    fn moves_refuse_collisions_and_roots_but_allow_case_renames() {
        let f = fixture();
        fs::write(f.project.join("Readme.md"), "r").unwrap();
        fs::write(f.project.join("other"), "o").unwrap();
        assert!(matches!(
            f.folders.move_entry(&s(&f.project.join("other")), &s(&f.project.join("Readme.md"))),
            Err(FoldersError::InvalidArgument(_))
        ));
        f.folders.move_entry(&s(&f.project.join("Readme.md")), &s(&f.project.join("README.md"))).unwrap();
        assert!(fs::read_dir(&f.project).unwrap().flatten().any(|e| e.file_name() == "README.md"));
        assert!(matches!(f.folders.move_entry(&s(&f.project), &s(&f.project.join("x"))), Err(FoldersError::InvalidArgument(_))));
        assert!(matches!(f.folders.delete(&s(&f.project), false), Err(FoldersError::InvalidArgument(_))));
    }

    #[test]
    fn creates_and_deletes_folders_idempotently() {
        let f = fixture();
        let deep = f.project.join(".lazify/api-studio/docs");
        f.folders.create_folder(&s(&deep)).unwrap();
        f.folders.create_folder(&s(&deep)).unwrap();
        fs::write(deep.join("draft.md"), "d").unwrap();
        f.folders.delete(&s(&f.project.join(".lazify")), false).unwrap();
        f.folders.delete(&s(&f.project.join(".lazify")), false).unwrap();
        assert!(!f.project.join(".lazify").exists());
    }

    #[test]
    fn working_directory_must_be_an_existing_granted_folder() {
        let f = fixture();
        assert_eq!(f.folders.working_directory(&s(&f.project.join("src"))).unwrap(), f.project.join("src"));
        assert!(f.folders.working_directory(&s(&f.base.join("declared"))).is_ok());
        assert!(matches!(f.folders.working_directory(&s(&f.base.join("outside"))), Err(FoldersError::NotGranted(_))));
        assert!(matches!(f.folders.working_directory(&s(&f.project.join("nope"))), Err(FoldersError::NotFound(_))));
    }

    #[test]
    fn app_folders_are_read_write_grants_but_not_listed() {
        let f = fixture();
        let folders = Folders::open(&f.base.join("data/grants.json"), &[], None)
            .unwrap()
            .with_app_folders(&f.base.join("data/app"), &f.base.join("tmp/app"))
            .unwrap();
        let data = folders.app_folder(AppFolder::Data).unwrap();
        assert_eq!((data.id.as_str(), data.source, data.access), ("app:data", GrantSource::App, Access::ReadWrite));
        assert!(folders.grants().is_empty());

        let repo = format!("{}/shadow-repos/x/info", data.path);
        folders.create_folder(&repo).unwrap();
        folders.write(&format!("{repo}/exclude"), b"node_modules\n").unwrap();
        assert_eq!(folders.working_directory(&repo).unwrap(), PathBuf::from(&repo));
        // The grants file sits beside the data folder, out of reach.
        assert!(matches!(folders.write(&s(&f.base.join("data/grants.json")), b"[]"), Err(FoldersError::NotGranted(_))));
        assert!(matches!(folders.delete(&data.path, false), Err(FoldersError::InvalidArgument(_))));

        let temp = folders.app_folder(AppFolder::Temp).unwrap();
        fs::remove_dir_all(&temp.path).unwrap();
        folders.app_folder(AppFolder::Temp).unwrap();
        assert!(Path::new(&temp.path).is_dir(), "a cleared temp folder is recreated");

        let enclosing = Folders::open(&f.base.join("data/grants.json"), &[], None).unwrap().with_app_folders(&f.base.join("data"), &f.base.join("tmp/app"));
        assert!(matches!(enclosing, Err(FoldersError::InvalidArgument(_))));
    }

    #[test]
    fn watch_reports_changes_until_dropped() {
        let f = fixture();
        let (tx, rx) = mpsc::channel();
        let watch = f.folders.watch(&s(&f.project), true, move |changes| {
            let _ = tx.send(changes);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        fs::write(f.project.join("src/new.txt"), "n").unwrap();

        let target = s(&f.project.join("src/new.txt"));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut seen = false;
        while !seen && std::time::Instant::now() < deadline {
            if let Ok(changes) = rx.recv_timeout(Duration::from_millis(200)) {
                seen = changes.iter().any(|c| c.path == target);
            }
        }
        assert!(seen, "no change event for {target}");
        drop(watch);
    }
}
