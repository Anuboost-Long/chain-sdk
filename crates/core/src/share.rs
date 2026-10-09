//! Share capability — see /agent-docs/capabilities/share/CONTRACT.md.
//! Each share copies its files into a folder of its own, under the names
//! the recipient should see, so the app may delete its files as soon as
//! show() resolves; swift/ChainShare.swift shows the menu. The folder is
//! deleted on cancel, else by `sweep` a day later: a picked service may
//! hold only the files' URLs (Copy puts them on the pasteboard).

use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::files::Files;

/// Folders a service never reported finishing with are deleted after this.
pub const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_NAME_BYTES: usize = 200;

#[derive(Debug)]
pub enum ShareError {
    InvalidArgument(String),
    NotFound(String),
    /// A share menu is already open.
    Unavailable(String),
    Unsupported(String),
    Other(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedFile {
    reference: String,
    name: Option<String>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// `ShareOptions` from contract.ts.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShareOptions {
    anchor: Option<Anchor>,
    text: Option<String>,
    title: Option<String>,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ShareResult {
    Picked { service: Option<String> },
    Cancelled,
}

/// `ShareAvailability` from contract.ts.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    available: bool,
    anchor: bool,
    text: bool,
    title: bool,
    service_name: bool,
}

pub fn availability() -> Availability {
    let available = cfg!(target_os = "macos");
    Availability { available, anchor: available, text: available, title: available, service_name: available }
}

fn invalid(message: impl Into<String>) -> ShareError {
    ShareError::InvalidArgument(message.into())
}

pub fn parse_files(files: Value) -> Result<Vec<SharedFile>, ShareError> {
    let files: Vec<SharedFile> = serde_json::from_value(files).map_err(|e| invalid(format!("invalid files to share: {e}")))?;
    if files.is_empty() {
        return Err(invalid("there are no files to share"));
    }
    Ok(files)
}

impl ShareOptions {
    /// `options` as the SDK sent it; null or absent is all defaults.
    pub fn parse(options: Option<Value>) -> Result<Self, ShareError> {
        let options: Self = match options {
            None | Some(Value::Null) => Self::default(),
            Some(value) => serde_json::from_value(value).map_err(|e| invalid(format!("invalid share options: {e}")))?,
        };
        if let Some(Anchor { x, y, width, height }) = options.anchor {
            if ![x, y, width, height].iter().all(|v| v.is_finite()) || width < 0.0 || height < 0.0 {
                return Err(invalid("the anchor needs finite x and y and a width and height of 0 or more"));
            }
        }
        Ok(options)
    }
}

/// A copy of every file in a new folder under `root`, named for the
/// recipient. Returns the folder and the copies' paths, in order.
pub fn stage(files: &Files, root: &Path, shared: &[SharedFile]) -> Result<(PathBuf, Vec<PathBuf>), ShareError> {
    let sources = shared
        .iter()
        .map(|file| {
            files.resolve(&file.reference).map_err(|_| ShareError::NotFound(format!("no such file: {}", file.reference)))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let folder = root.join(crate::files::generate_id());
    fs::create_dir_all(&folder).map_err(|e| ShareError::Other(format!("couldn't prepare the files to share: {e}")))?;
    let mut copies: Vec<PathBuf> = Vec::with_capacity(shared.len());
    for (file, source) in shared.iter().zip(sources) {
        let name = recipient_name(file.name.as_deref(), &file.reference);
        let copy = unique_path(&folder, &name, &copies);
        // A clone on APFS: no extra space, however big the file.
        if let Err(e) = fs::copy(&source, &copy) {
            let _ = fs::remove_dir_all(&folder);
            return Err(ShareError::Other(format!("couldn't prepare {name} to share: {e}")));
        }
        copies.push(copy);
    }
    Ok((folder, copies))
}

/// Deletes the folders under `root` older than `STALE_AFTER`.
pub fn sweep(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let modified = entry.metadata().and_then(|m| m.modified()).ok();
        let age = modified.and_then(|at| SystemTime::now().duration_since(at).ok());
        if age.is_some_and(|age| age > STALE_AFTER) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// The name the recipient sees: the app's name made safe for a file
/// name, else the reference; the reference's extension added when the
/// name has none.
fn recipient_name(name: Option<&str>, reference: &str) -> String {
    let cleaned: String = name
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':') { '-' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim();
    if cleaned.is_empty() {
        return reference.to_string();
    }
    let extension = Path::new(reference).extension().map(|e| e.to_string_lossy().into_owned());
    let has_extension = Path::new(cleaned).extension().is_some();
    let suffix = match extension {
        Some(ext) if !has_extension => format!(".{ext}"),
        _ => String::new(),
    };
    let mut stem = cleaned.to_string();
    let limit = MAX_NAME_BYTES - suffix.len();
    if stem.len() > limit {
        let mut end = limit;
        while !stem.is_char_boundary(end) {
            end -= 1;
        }
        stem.truncate(end);
    }
    stem + &suffix
}

/// `name` in `folder`, or "name 2", "name 3"... when an earlier file took it.
fn unique_path(folder: &Path, name: &str, taken: &[PathBuf]) -> PathBuf {
    let path = folder.join(name);
    if !taken.contains(&path) {
        return path;
    }
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (2..).map(|n| folder.join(format!("{stem} {n}{extension}"))).find(|path| !taken.contains(path)).expect("unbounded")
}

pub type ShowDone = Box<dyn FnOnce(Result<ShareResult, ShareError>) + Send>;

/// One share menu at a time per app.
static MENU_OPEN: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
mod native {
    use std::ffi::{c_char, CStr, CString};

    use super::*;

    const STATUS_PICKED: i32 = 0;

    type Finish = extern "C" fn(*mut c_void, i32, *const c_char);

    extern "C" {
        fn chain_share_show(webview: *mut c_void, request_json: *const c_char, context: *mut c_void, on_finish: Finish) -> bool;
    }

    #[derive(Serialize)]
    struct Request<'a> {
        paths: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        anchor: Option<Anchor>,
        folder: String,
    }

    /// `context` is a leaked `ShowDone`; Swift calls back exactly once.
    extern "C" fn forward_finish(context: *mut c_void, status: i32, payload: *const c_char) {
        let done = unsafe { Box::from_raw(context.cast::<ShowDone>()) };
        let service = unsafe { CStr::from_ptr(payload) }.to_string_lossy().into_owned();
        MENU_OPEN.store(false, Ordering::SeqCst);
        done(Ok(match status {
            STATUS_PICKED => ShareResult::Picked { service: Some(service).filter(|s| !s.is_empty()) },
            _ => ShareResult::Cancelled,
        }));
    }

    /// Main thread, from `with_webview`: `webview` is the calling page's
    /// WKWebView. `done` runs when the menu closes, on the main thread.
    pub fn show(webview: *mut c_void, folder: &Path, copies: &[PathBuf], options: &ShareOptions, done: ShowDone) {
        let path = |p: &Path| p.to_string_lossy().into_owned();
        let request = Request {
            paths: copies.iter().map(|p| path(p)).collect(),
            text: options.text.as_deref().filter(|t| !t.is_empty()),
            title: options.title.as_deref(),
            anchor: options.anchor,
            folder: path(folder),
        };
        if MENU_OPEN.swap(true, Ordering::SeqCst) {
            let _ = fs::remove_dir_all(folder);
            return done(Err(ShareError::Unavailable("a share menu is already open".to_string())));
        }
        let json = CString::new(serde_json::to_string(&request).expect("the request serializes")).expect("JSON has no NUL");
        let context = Box::into_raw(Box::new(done));
        if !unsafe { chain_share_show(webview, json.as_ptr(), context.cast(), forward_finish) } {
            MENU_OPEN.store(false, Ordering::SeqCst);
            let _ = fs::remove_dir_all(folder);
            let done = unsafe { Box::from_raw(context) };
            done(Err(ShareError::Other("couldn't open the share menu".to_string())));
        }
    }
}

#[cfg(target_os = "macos")]
pub use native::show;

#[cfg(not(target_os = "macos"))]
pub fn show(_webview: *mut c_void, folder: &Path, _copies: &[PathBuf], _options: &ShareOptions, done: ShowDone) {
    let _ = fs::remove_dir_all(folder);
    done(Err(ShareError::Unsupported("the share menu isn't available on this platform yet".to_string())));
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("chain-share-test-{}", crate::files::generate_id()))
    }

    #[test]
    fn names_files_for_the_recipient() {
        assert_eq!(recipient_name(Some("Cell biology summary"), "0123abcd.pdf"), "Cell biology summary.pdf");
        assert_eq!(recipient_name(Some("Notes.pdf"), "0123abcd.pdf"), "Notes.pdf");
        assert_eq!(recipient_name(Some("Week 1/2: cells"), "0123abcd.pdf"), "Week 1-2- cells.pdf");
        assert_eq!(recipient_name(Some("  .hidden "), "0123abcd.pdf"), "hidden.pdf");
        assert_eq!(recipient_name(Some(" "), "0123abcd.pdf"), "0123abcd.pdf");
        assert_eq!(recipient_name(None, "0123abcd"), "0123abcd");
        let long = recipient_name(Some(&"ក".repeat(100)), "a.pdf");
        assert!(long.len() <= MAX_NAME_BYTES && long.ends_with("ក.pdf"));
    }

    #[test]
    fn stages_copies_under_unique_names() {
        let dir = temp_dir();
        let files = Files::open(&dir.join("files")).unwrap();
        let a = files.write(b"first", Some("pdf")).unwrap();
        let b = files.write(b"second", Some("pdf")).unwrap();
        let shared = parse_files(json!([
            { "reference": a, "name": "Summary" },
            { "reference": b, "name": "Summary.pdf" },
        ]))
        .unwrap();

        let (folder, copies) = stage(&files, &dir.join("share"), &shared).unwrap();
        let names: Vec<_> = copies.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["Summary.pdf", "Summary 2.pdf"]);
        assert_eq!(fs::read(&copies[1]).unwrap(), b"second");
        // The app may delete its own file straight away.
        files.delete(&a).unwrap();
        assert_eq!(fs::read(&copies[0]).unwrap(), b"first");
        assert!(copies.iter().all(|p| p.starts_with(&folder)));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_missing_files_and_bad_options() {
        let dir = temp_dir();
        let files = Files::open(&dir.join("files")).unwrap();
        let missing = parse_files(json!([{ "reference": "0000000000000000.pdf" }])).unwrap();
        assert!(matches!(stage(&files, &dir.join("share"), &missing), Err(ShareError::NotFound(_))));
        let escape = parse_files(json!([{ "reference": "../secret" }])).unwrap();
        assert!(matches!(stage(&files, &dir.join("share"), &escape), Err(ShareError::NotFound(_))));
        assert!(!dir.join("share").exists());

        assert!(matches!(parse_files(json!([])), Err(ShareError::InvalidArgument(_))));
        assert!(matches!(parse_files(json!([{ "path": "/etc/hosts" }])), Err(ShareError::InvalidArgument(_))));
        let bad_anchor = json!({ "anchor": { "x": 0, "y": 0, "width": -1, "height": 10 } });
        assert!(matches!(ShareOptions::parse(Some(bad_anchor)), Err(ShareError::InvalidArgument(_))));
        assert!(ShareOptions::parse(Some(json!({ "anchor": { "x": 1, "y": 2, "width": 30, "height": 20 }, "title": "t" }))).is_ok());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sweeps_only_stale_folders() {
        let root = temp_dir();
        fs::create_dir_all(root.join("fresh")).unwrap();
        sweep(&root);
        assert!(root.join("fresh").exists());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn result_serializes_as_the_contract_says() {
        let picked = ShareResult::Picked { service: Some("AirDrop".to_string()) };
        assert_eq!(serde_json::to_value(picked).unwrap(), json!({ "status": "picked", "service": "AirDrop" }));
        assert_eq!(serde_json::to_value(ShareResult::Cancelled).unwrap(), json!({ "status": "cancelled" }));
    }
}
