//! Models capability — see /agent-docs/capabilities/models/CONTRACT.md.
//! Downloads data-only model packs (ONNX weights, token lists, lexicons)
//! into a managed directory: streamed to a temp file, SHA-256 checked
//! before anything is kept, extracted with only plain files and folders
//! allowed, then moved into place. Nothing downloaded is ever executed or
//! marked executable. Plain Rust — the same on every OS.

use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug)]
pub enum ModelsError {
    InvalidArgument(String),
    NotFound(String),
    /// Network failure, or an install of this id is already running.
    Unavailable(String),
    /// The download doesn't match the manifest's SHA-256.
    Integrity(String),
    Cancelled,
    Other(String),
}

impl From<io::Error> for ModelsError {
    fn from(e: io::Error) -> Self {
        ModelsError::Other(e.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ArchiveKind {
    #[serde(rename = "tar.bz2")]
    TarBz2,
    #[serde(rename = "tar.gz")]
    TarGz,
    #[serde(rename = "zip")]
    Zip,
    #[serde(rename = "none")]
    None,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelManifest {
    pub id: String,
    pub url: String,
    pub sha256: String,
    pub archive: Option<ArchiveKind>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledModel {
    pub id: String,
    pub size_bytes: u64,
    pub installed_at: String,
}

/// Kept beside each model, never inside it, so no archive can overwrite it.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    installed_at: String,
    url: String,
    sha256: String,
}

const META_DIR: &str = ".meta";
const PARTIAL_DIR: &str = ".partial";
const TRASH_DIR: &str = ".trash";

pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.starts_with('-')
}

impl ModelManifest {
    fn validate(&self) -> Result<ArchiveKind, ModelsError> {
        let invalid = |m: String| Err(ModelsError::InvalidArgument(m));
        if !is_valid_id(&self.id) {
            return invalid(format!("model id {:?} must be 1–64 of a–z, 0–9 and '-'", self.id));
        }
        match reqwest::Url::parse(&self.url) {
            Ok(url) if url.scheme() == "https" => {}
            _ => return invalid(format!("model url must be an https:// URL, got {:?}", self.url)),
        }
        if self.sha256.len() != 64 || !self.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return invalid("sha256 must be 64 hex characters".to_string());
        }
        Ok(self.archive.unwrap_or_else(|| infer_archive(&self.url)))
    }
}

fn infer_archive(url: &str) -> ArchiveKind {
    let path = url.split(['?', '#']).next().unwrap_or_default().to_ascii_lowercase();
    if path.ends_with(".tar.bz2") || path.ends_with(".tbz2") {
        ArchiveKind::TarBz2
    } else if path.ends_with(".tar.gz") || path.ends_with(".tgz") {
        ArchiveKind::TarGz
    } else if path.ends_with(".zip") {
        ArchiveKind::Zip
    } else {
        ArchiveKind::None
    }
}

/// One install per id at a time; `cancel(id)` flips that install's flag.
/// Cheap to clone — every clone is the same registry.
#[derive(Default, Clone)]
pub struct Installs(Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>);

/// Owned (not borrowed), so an install can move to a blocking thread.
pub struct InstallTicket {
    installs: Installs,
    id: String,
    cancelled: Arc<AtomicBool>,
}

impl Installs {
    pub fn begin(&self, id: &str) -> Result<InstallTicket, ModelsError> {
        let mut running = self.0.lock().expect("installs mutex poisoned");
        if running.contains_key(id) {
            return Err(ModelsError::Unavailable(format!("model {id} is already being installed")));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        running.insert(id.to_string(), Arc::clone(&cancelled));
        Ok(InstallTicket { installs: self.clone(), id: id.to_string(), cancelled })
    }

    /// No-op when nothing with that id is installing.
    pub fn cancel(&self, id: &str) {
        if let Some(flag) = self.0.lock().expect("installs mutex poisoned").get(id) {
            flag.store(true, Ordering::SeqCst);
        }
    }

    pub fn is_installing(&self, id: &str) -> bool {
        self.0.lock().expect("installs mutex poisoned").contains_key(id)
    }
}

impl InstallTicket {
    fn check(&self) -> Result<(), ModelsError> {
        if self.cancelled.load(Ordering::SeqCst) {
            Err(ModelsError::Cancelled)
        } else {
            Ok(())
        }
    }
}

impl Drop for InstallTicket {
    fn drop(&mut self) {
        self.installs.0.lock().expect("installs mutex poisoned").remove(&self.id);
    }
}

/// A verified download waiting to be unpacked. Its file is removed if it's
/// dropped without being installed.
pub struct Download {
    path: PathBuf,
    kind: ArchiveKind,
    manifest: ModelManifest,
}

impl Drop for Download {
    fn drop(&mut self) {
        fs::remove_file(&self.path).ok();
    }
}

#[derive(Clone)]
pub struct Models {
    dir: PathBuf,
}

impl Models {
    pub fn open(dir: &Path) -> Result<Self, ModelsError> {
        for sub in [META_DIR, PARTIAL_DIR, TRASH_DIR] {
            fs::create_dir_all(dir.join(sub))?;
        }
        Ok(Self { dir: dir.to_path_buf() })
    }

    /// Streams `manifest.url` to a temp file, hashing as it goes, and
    /// rejects it unless the SHA-256 matches. `on_progress(received, total)`
    /// fires at most every 100 ms; `total` is Content-Length, else the
    /// manifest's `sizeBytes`.
    pub async fn download(
        &self,
        manifest: &ModelManifest,
        ticket: &InstallTicket,
        mut on_progress: impl FnMut(u64, Option<u64>),
    ) -> Result<Download, ModelsError> {
        let kind = manifest.validate()?;
        let unavailable = |e: reqwest::Error| ModelsError::Unavailable(format!("couldn't download the model: {e}"));
        let mut response = reqwest::get(&manifest.url).await.map_err(unavailable)?;
        if !response.status().is_success() {
            return Err(ModelsError::Unavailable(format!(
                "couldn't download the model: the server answered {}",
                response.status()
            )));
        }
        let total = response.content_length().or(manifest.size_bytes);

        let download = Download {
            path: self.dir.join(PARTIAL_DIR).join(format!("{}.{}.download", manifest.id, random_suffix())),
            kind,
            manifest: manifest.clone(),
        };
        let mut file = fs::File::create(&download.path)?;
        let mut hasher = Sha256::new();
        let mut received = 0u64;
        let mut last_report = Instant::now() - Duration::from_secs(1);
        on_progress(0, total);
        while let Some(chunk) = response.chunk().await.map_err(unavailable)? {
            ticket.check()?;
            file.write_all(&chunk)?;
            hasher.update(&chunk);
            received += chunk.len() as u64;
            if last_report.elapsed() >= Duration::from_millis(100) {
                on_progress(received, total);
                last_report = Instant::now();
            }
        }
        file.sync_all()?;
        on_progress(received, total);

        let actual = hex(&hasher.finalize());
        if !actual.eq_ignore_ascii_case(&manifest.sha256) {
            return Err(ModelsError::Integrity(format!(
                "the download's SHA-256 is {actual}, not the expected {} — nothing was kept",
                manifest.sha256.to_ascii_lowercase()
            )));
        }
        Ok(download)
    }

    /// Unpacks a verified download into a staging folder, then swaps it in
    /// place of any earlier install of the same id. Blocking.
    pub fn install(&self, download: Download, ticket: &InstallTicket) -> Result<InstalledModel, ModelsError> {
        let id = download.manifest.id.clone();
        let staging = self.dir.join(PARTIAL_DIR).join(format!("{id}.{}", random_suffix()));
        fs::create_dir_all(&staging)?;
        let unpacked = unpack(&download, &staging, ticket).and_then(|()| flatten_single_folder(&staging));
        if let Err(e) = unpacked {
            fs::remove_dir_all(&staging).ok();
            return Err(e);
        }
        ticket.check().inspect_err(|_| {
            fs::remove_dir_all(&staging).ok();
        })?;

        let target = self.dir.join(&id);
        let trash = self.dir.join(TRASH_DIR).join(format!("{id}.{}", random_suffix()));
        if target.exists() {
            fs::rename(&target, &trash)?;
        }
        fs::rename(&staging, &target)?;
        fs::remove_dir_all(&trash).ok();

        let metadata = Metadata {
            installed_at: rfc3339(SystemTime::now()),
            url: download.manifest.url.clone(),
            sha256: download.manifest.sha256.to_ascii_lowercase(),
        };
        fs::write(self.meta_path(&id), serde_json::to_vec(&metadata).map_err(|e| ModelsError::Other(e.to_string()))?)?;
        Ok(InstalledModel { id: id.clone(), size_bytes: dir_size(&target), installed_at: metadata.installed_at })
    }

    pub fn list(&self) -> Result<Vec<InstalledModel>, ModelsError> {
        let mut models = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let id = entry.file_name().to_string_lossy().into_owned();
            if !is_valid_id(&id) || !entry.file_type()?.is_dir() {
                continue;
            }
            let Some(metadata) = fs::read(self.meta_path(&id)).ok().and_then(|b| serde_json::from_slice::<Metadata>(&b).ok())
            else {
                continue;
            };
            models.push(InstalledModel { size_bytes: dir_size(&entry.path()), id, installed_at: metadata.installed_at });
        }
        models.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(models)
    }

    /// Idempotent: removing a model that isn't installed succeeds.
    pub fn remove(&self, id: &str) -> Result<(), ModelsError> {
        if !is_valid_id(id) {
            return Err(ModelsError::InvalidArgument(format!("invalid model id {id:?}")));
        }
        let target = self.dir.join(id);
        if target.exists() {
            let trash = self.dir.join(TRASH_DIR).join(format!("{id}.{}", random_suffix()));
            fs::rename(&target, &trash)?;
            fs::remove_dir_all(&trash)?;
        }
        match fs::remove_file(self.meta_path(id)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    /// The path of `relative` inside an installed model — for engines only;
    /// never handed to the app. `NotFound` when the model or file is absent.
    pub fn resolve(&self, id: &str, relative: &str) -> Result<PathBuf, ModelsError> {
        let path = self.inside(id, relative)?;
        if !path.is_file() {
            return Err(ModelsError::NotFound(format!("model {id} has no file {relative:?}")));
        }
        Ok(path)
    }

    /// Like `resolve`, for a folder (e.g. a voice's `espeak-ng-data`).
    pub fn resolve_dir(&self, id: &str, relative: &str) -> Result<PathBuf, ModelsError> {
        let path = self.inside(id, relative)?;
        if !path.is_dir() {
            return Err(ModelsError::NotFound(format!("model {id} has no folder {relative:?}")));
        }
        Ok(path)
    }

    fn inside(&self, id: &str, relative: &str) -> Result<PathBuf, ModelsError> {
        if !is_valid_id(id) {
            return Err(ModelsError::InvalidArgument(format!("invalid model id {id:?}")));
        }
        if !self.dir.join(id).is_dir() {
            return Err(ModelsError::NotFound(format!("model {id} isn't installed")));
        }
        let relative_path = Path::new(relative);
        if relative.is_empty() || !relative_path.components().all(|c| matches!(c, Component::Normal(_))) {
            return Err(ModelsError::InvalidArgument(format!("{relative:?} isn't a name inside the model")));
        }
        Ok(self.dir.join(id).join(relative_path))
    }

    fn meta_path(&self, id: &str) -> PathBuf {
        self.dir.join(META_DIR).join(format!("{id}.json"))
    }
}

/// A relative path made only of normal components, or `None` for anything
/// that could land outside `root` (absolute, `..`, drive prefixes).
fn safe_relative(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

fn unsafe_archive(what: &str) -> ModelsError {
    ModelsError::InvalidArgument(format!("the archive {what} — only plain files and folders inside the model are allowed"))
}

/// Copies `reader` to a new file (never following or replacing anything),
/// with default permissions — nothing is made executable.
fn write_new_file(path: &Path, reader: &mut impl Read) -> Result<(), ModelsError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    io::copy(reader, &mut file)?;
    Ok(())
}

fn unpack(download: &Download, root: &Path, ticket: &InstallTicket) -> Result<(), ModelsError> {
    let file = fs::File::open(&download.path)?;
    match download.kind {
        ArchiveKind::TarBz2 => unpack_tar(tar::Archive::new(bzip2::read::BzDecoder::new(file)), root, ticket),
        ArchiveKind::TarGz => unpack_tar(tar::Archive::new(flate2::read::GzDecoder::new(file)), root, ticket),
        ArchiveKind::Zip => unpack_zip(file, root, ticket),
        ArchiveKind::None => {
            let name = file_name_from_url(&download.manifest.url);
            write_new_file(&root.join(name), &mut io::BufReader::new(file))
        }
    }
}

fn unpack_tar<R: Read>(mut archive: tar::Archive<R>, root: &Path, ticket: &InstallTicket) -> Result<(), ModelsError> {
    for entry in archive.entries()? {
        ticket.check()?;
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let relative = safe_relative(&path).ok_or_else(|| unsafe_archive(&format!("has a path outside the model ({})", path.display())))?;
        match entry.header().entry_type() {
            tar::EntryType::Regular | tar::EntryType::Continuous => write_new_file(&root.join(relative), &mut entry)?,
            tar::EntryType::Directory => fs::create_dir_all(root.join(relative))?,
            // pax/GNU metadata records describe other entries; they aren't files.
            tar::EntryType::XGlobalHeader | tar::EntryType::XHeader | tar::EntryType::GNULongName | tar::EntryType::GNULongLink => {}
            other => return Err(unsafe_archive(&format!("contains a {other:?} entry ({})", path.display()))),
        }
    }
    Ok(())
}

fn unpack_zip(file: fs::File, root: &Path, ticket: &InstallTicket) -> Result<(), ModelsError> {
    let mut archive = zip::ZipArchive::new(file).map_err(|e| ModelsError::InvalidArgument(format!("not a readable zip: {e}")))?;
    for index in 0..archive.len() {
        ticket.check()?;
        let mut entry = archive.by_index(index).map_err(|e| ModelsError::Other(e.to_string()))?;
        let name = entry.name().to_string();
        if entry.is_symlink() {
            return Err(unsafe_archive(&format!("contains a link ({name})")));
        }
        let relative = entry
            .enclosed_name()
            .and_then(|p| safe_relative(&p))
            .ok_or_else(|| unsafe_archive(&format!("has a path outside the model ({name})")))?;
        if entry.is_dir() {
            fs::create_dir_all(root.join(relative))?;
        } else {
            write_new_file(&root.join(relative), &mut entry)?;
        }
    }
    Ok(())
}

/// Release archives usually wrap everything in one top folder
/// (`sherpa-onnx-whisper-tiny.en/…`); the app's catalog names files
/// relative to the model itself, so that folder is lifted out.
fn flatten_single_folder(root: &Path) -> Result<(), ModelsError> {
    let entries: Vec<_> = fs::read_dir(root)?.collect::<Result<_, _>>()?;
    let [only] = entries.as_slice() else { return Ok(()) };
    if !only.file_type()?.is_dir() {
        return Ok(());
    }
    let inner = only.path();
    let lifted = root.with_extension("lift");
    fs::rename(&inner, &lifted)?;
    fs::remove_dir(root)?;
    fs::rename(&lifted, root)?;
    Ok(())
}

fn file_name_from_url(url: &str) -> String {
    let last = url.split(['?', '#']).next().unwrap_or_default().rsplit('/').next().unwrap_or_default();
    let clean: String = last.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).collect();
    if clean.is_empty() || clean.starts_with('.') {
        "model".to_string()
    } else {
        clean
    }
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(t) if t.is_dir() => dir_size(&entry.path()),
            Ok(t) if t.is_file() => entry.metadata().map_or(0, |m| m.len()),
            _ => 0,
        })
        .sum()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn random_suffix() -> String {
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    format!("{:08x}", hasher.finish() as u32)
}

/// UTC, second precision: `2026-09-28T14:03:07Z`.
fn rfc3339(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Howard Hinnant's days-to-civil.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("chain-models-test-{}", random_suffix()))
    }

    fn manifest(url: &str) -> ModelManifest {
        ModelManifest { id: "tiny-model".to_string(), url: url.to_string(), sha256: "a".repeat(64), archive: None, size_bytes: None }
    }

    fn download_of(models: &Models, bytes: &[u8], kind: ArchiveKind, url: &str) -> Download {
        let path = models.dir.join(PARTIAL_DIR).join(format!("t.{}", random_suffix()));
        fs::write(&path, bytes).unwrap();
        Download { path, kind, manifest: manifest(url) }
    }

    fn tar_with(build: impl FnOnce(&mut tar::Builder<Vec<u8>>)) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        build(&mut builder);
        builder.into_inner().unwrap()
    }

    fn bz2(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn add_file(builder: &mut tar::Builder<Vec<u8>>, path: &str, data: &[u8]) {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o755);
        header.set_entry_type(tar::EntryType::Regular);
        // set_path refuses "..", so write the raw name the way a hostile archive would.
        let name = &mut header.as_old_mut().name;
        name[..path.len()].copy_from_slice(path.as_bytes());
        header.set_cksum();
        builder.append(&header, data).unwrap();
    }

    #[test]
    fn validates_manifests() {
        assert_eq!(manifest("https://x.dev/m.tar.bz2").validate().unwrap(), ArchiveKind::TarBz2);
        assert_eq!(manifest("https://x.dev/m.onnx?dl=1").validate().unwrap(), ArchiveKind::None);
        assert!(matches!(manifest("http://x.dev/m.zip").validate(), Err(ModelsError::InvalidArgument(_))));
        let mut bad = manifest("https://x.dev/m.zip");
        bad.id = "Bad_Id".to_string();
        assert!(bad.validate().is_err());
        bad.id = "ok".to_string();
        bad.sha256 = "xyz".to_string();
        assert!(bad.validate().is_err());
    }

    #[test]
    fn installs_lists_resolves_and_removes_a_tar() {
        let dir = temp_dir();
        let models = Models::open(&dir).unwrap();
        let installs = Installs::default();
        let bytes = tar_with(|b| {
            add_file(b, "pack/tokens.txt", b"a b c");
            add_file(b, "pack/sub/model.onnx", b"weights");
        });
        let ticket = installs.begin("tiny-model").unwrap();
        let installed = models.install(download_of(&models, &bz2(&bytes), ArchiveKind::TarBz2, "https://x/p.tar.bz2"), &ticket).unwrap();
        drop(ticket);
        assert_eq!(installed.size_bytes, 12);

        // The single "pack/" folder was lifted out; nothing is executable.
        let path = models.resolve("tiny-model", "sub/model.onnx").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"weights");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o111, 0);
        }
        assert!(matches!(models.resolve("tiny-model", "../x"), Err(ModelsError::InvalidArgument(_))));
        assert!(matches!(models.resolve("tiny-model", "missing.onnx"), Err(ModelsError::NotFound(_))));
        assert_eq!(models.list().unwrap().len(), 1);

        models.remove("tiny-model").unwrap();
        models.remove("tiny-model").unwrap();
        assert!(models.list().unwrap().is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_escaping_paths_and_links_and_keeps_nothing() {
        let dir = temp_dir();
        let models = Models::open(&dir).unwrap();
        let installs = Installs::default();
        let ticket = installs.begin("tiny-model").unwrap();

        let escaping = tar_with(|b| add_file(b, "../evil.txt", b"x"));
        let result = models.install(download_of(&models, &bz2(&escaping), ArchiveKind::TarBz2, "https://x/p.tar.bz2"), &ticket);
        assert!(matches!(result, Err(ModelsError::InvalidArgument(m)) if m.contains("outside")));

        let linked = tar_with(|b| {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            b.append_link(&mut header, "link", "/etc/passwd").unwrap();
        });
        let result = models.install(download_of(&models, &bz2(&linked), ArchiveKind::TarBz2, "https://x/p.tar.bz2"), &ticket);
        assert!(matches!(result, Err(ModelsError::InvalidArgument(m)) if m.contains("Symlink")));

        assert!(!dir.join("tiny-model").exists());
        assert!(!dir.parent().unwrap().join("evil.txt").exists());
        assert_eq!(fs::read_dir(dir.join(PARTIAL_DIR)).unwrap().count(), 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn one_install_per_id_and_cancel_flags_it() {
        let installs = Installs::default();
        let ticket = installs.begin("a").unwrap();
        assert!(matches!(installs.begin("a"), Err(ModelsError::Unavailable(_))));
        installs.cancel("a");
        assert!(matches!(ticket.check(), Err(ModelsError::Cancelled)));
        drop(ticket);
        assert!(installs.begin("a").is_ok());
    }

    #[test]
    fn formats_utc_timestamps() {
        assert_eq!(rfc3339(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(UNIX_EPOCH + Duration::from_secs(1_790_604_187)), "2026-09-28T14:03:07Z");
    }
}
