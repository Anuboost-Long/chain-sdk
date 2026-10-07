//! Browser capability — see /agent-docs/capabilities/browser/CONTRACT.md.
//! The Tauri-free half of the signed-in browser window: session naming,
//! where the page may navigate, the page-reading script, cookie matching
//! and the session fetch. The window itself is built in the Tauri layer
//! (packages/cli/templates/browser.rs), which hands this module plain data.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::http::{self, HttpError};

#[derive(Debug)]
pub enum BrowserError {
    InvalidArgument(String),
    NotFound(String),
    Unavailable(String),
    Unsupported(String),
    Other(String),
}

pub const DEFAULT_SESSION: &str = "default";
const MAX_SESSION_CHARS: usize = 100;

/// The session name to use: `"default"` when none is given.
pub fn session_name(session: Option<&str>) -> Result<String, BrowserError> {
    let name = session.unwrap_or(DEFAULT_SESSION);
    let chars = name.chars().count();
    if chars == 0 || chars > MAX_SESSION_CHARS {
        return Err(BrowserError::InvalidArgument(format!(
            "session must be 1–{MAX_SESSION_CHARS} characters, got {chars}"
        )));
    }
    Ok(name.to_string())
}

/// A session's stable 16 bytes: the macOS data store's UUID, and the
/// source of its window labels and Windows data folder. A UUIDv8 so
/// WebKit never sees the all-zero identifier it refuses.
pub fn session_key(session: &str) -> [u8; 16] {
    let digest = Sha256::digest(format!("chain-browser\0{session}").as_bytes());
    let mut key: [u8; 16] = digest[..16].try_into().expect("SHA-256 is 32 bytes");
    key[6] = (key[6] & 0x0f) | 0x80;
    key[8] = (key[8] & 0x3f) | 0x80;
    key
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The window's Tauri label; its webviews append `-page`, `-toolbar`, `-popup-N`.
pub fn session_label(session: &str) -> String {
    format!("chain-browser-{}", hex(&session_key(session)))
}

pub const LABEL_PREFIX: &str = "chain-browser-";

/// Where WebView2 keeps the session (unused on macOS, which keys by `session_key`).
pub fn session_data_dir(app_local_data: &Path, session: &str) -> PathBuf {
    app_local_data.join("browser-sessions").join(hex(&session_key(session)))
}

/// An address the app may open or fetch: http or https only.
pub fn web_url(url: &str) -> Result<reqwest::Url, BrowserError> {
    let parsed = reqwest::Url::parse(url).map_err(|e| BrowserError::InvalidArgument(format!("invalid URL {url:?}: {e}")))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        scheme => Err(BrowserError::InvalidArgument(format!("only http and https addresses can be opened, got {scheme}:"))),
    }
}

/// Whether the page may go to `url`. Never to the app's own origin — a
/// `*.localhost` custom scheme host or the dev server — since those may
/// call the app's commands.
pub fn may_navigate(url: &reqwest::Url, app_origin: Option<&reqwest::Url>) -> bool {
    match url.scheme() {
        "about" | "data" | "blob" => true,
        "http" | "https" => {
            let host = url.host_str().unwrap_or_default();
            let app = app_origin.is_some_and(|app| app.origin() == url.origin());
            !app && host != "localhost" && !host.ends_with(".localhost")
        }
        _ => false,
    }
}

/// An app-defined toolbar button.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Button {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

/// Ids must be non-empty and unique, labels non-empty.
pub fn validate_buttons(buttons: &[Button]) -> Result<(), BrowserError> {
    let invalid = |m: String| Err(BrowserError::InvalidArgument(m));
    for (i, button) in buttons.iter().enumerate() {
        if button.id.is_empty() {
            return invalid(format!("button {i} has an empty id"));
        }
        if button.label.trim().is_empty() {
            return invalid(format!("button {:?} has an empty label", button.id));
        }
        if buttons[..i].iter().any(|other| other.id == button.id) {
            return invalid(format!("button id {:?} is used twice", button.id));
        }
    }
    Ok(())
}

/// A window dimension in points: `default` when absent, else a positive number.
pub fn dimension(name: &str, value: Option<f64>, default: f64) -> Result<f64, BrowserError> {
    match value {
        None => Ok(default),
        Some(v) if v.is_finite() && v > 0.0 => Ok(v),
        Some(v) => Err(BrowserError::InvalidArgument(format!("{name} must be a positive number, got {v}"))),
    }
}

pub const TOOLBAR_HTML: &str = include_str!("browser_toolbar.html");

/// Runs in the page's main frame and returns a plain object (wry
/// JSON-serializes it). Frames are reached through `contentDocument`,
/// which is null for a cross-origin frame — the page's own rule.
pub const READ_SCRIPT: &str = r#"(() => {
  try {
    if (document.readyState === "loading") return { state: "loading" };
    if (!/html/.test(document.contentType)) return { state: "unsupported", contentType: document.contentType };
    const frames = [];
    const unreadableFrames = [];
    const visit = (doc) => {
      for (const element of doc.querySelectorAll("iframe, frame")) {
        let inner = null;
        try { inner = element.contentDocument; } catch (error) {}
        if (inner && inner.documentElement) {
          frames.push({ url: inner.location.href, html: inner.documentElement.outerHTML });
          visit(inner);
        } else {
          unreadableFrames.push(element.src || "about:blank");
        }
      }
    };
    visit(document);
    return {
      state: "ready",
      url: location.href,
      title: document.title,
      html: document.documentElement.outerHTML,
      frames,
      unreadableFrames
    };
  } catch (error) {
    return { state: "error", message: String(error) };
  }
})()"#;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Frame {
    pub url: String,
    pub html: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PageContent {
    pub url: String,
    pub title: String,
    pub html: String,
    pub frames: Vec<Frame>,
    pub unreadable_frames: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
enum ReadResult {
    Ready(PageContent),
    Loading,
    #[serde(rename_all = "camelCase")]
    Unsupported { content_type: String },
    Error { message: String },
}

/// Turns `READ_SCRIPT`'s JSON result into the page's content.
pub fn page_content(result: &str) -> Result<PageContent, BrowserError> {
    if result.is_empty() {
        return Err(BrowserError::Other("the page didn't return its content".to_string()));
    }
    match serde_json::from_str(result).map_err(|e| BrowserError::Other(format!("unexpected read result: {e}")))? {
        ReadResult::Ready(content) => Ok(content),
        ReadResult::Loading => Err(BrowserError::Unavailable("the page is still loading".to_string())),
        ReadResult::Unsupported { content_type } => {
            Err(BrowserError::Unsupported(format!("this page isn't an HTML document ({content_type})")))
        }
        ReadResult::Error { message } => Err(BrowserError::Other(format!("reading the page failed: {message}"))),
    }
}

/// One cookie from the session's store, as the web engine reports it.
#[derive(Debug, Clone)]
pub struct SessionCookie {
    pub name: String,
    pub value: String,
    /// Without a leading dot.
    pub domain: Option<String>,
    pub path: Option<String>,
    pub secure: bool,
    pub expires_unix: Option<i64>,
}

fn domain_matches(host: &str, domain: &str) -> bool {
    let domain = domain.trim_start_matches('.').to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    host == domain || (host.ends_with(&format!(".{domain}")) && host.parse::<std::net::IpAddr>().is_err())
}

fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    request_path == cookie_path
        || (request_path.starts_with(cookie_path)
            && (cookie_path.ends_with('/') || request_path[cookie_path.len()..].starts_with('/')))
}

/// The `Cookie` header the browser itself would send to `url` (RFC 6265
/// §5.4), longest paths first. None when no cookie applies.
pub fn cookie_header(cookies: &[SessionCookie], url: &reqwest::Url) -> Option<String> {
    let host = url.host_str()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let loopback = host == "localhost" || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback());
    let mut matching: Vec<&SessionCookie> = cookies
        .iter()
        .filter(|c| c.domain.as_deref().is_some_and(|d| domain_matches(host, d)))
        .filter(|c| path_matches(url.path(), c.path.as_deref().unwrap_or("/")))
        .filter(|c| !c.secure || url.scheme() == "https" || loopback)
        .filter(|c| c.expires_unix.is_none_or(|expires| expires > now))
        .collect();
    if matching.is_empty() {
        return None;
    }
    matching.sort_by_key(|c| std::cmp::Reverse(c.path.as_deref().unwrap_or("/").len()));
    Some(matching.iter().map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<_>>().join("; "))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FetchHead {
    url: String,
    status: u16,
    content_type: Option<String>,
}

pub struct SessionFetch<'a> {
    pub url: &'a str,
    pub cookies: &'a [SessionCookie],
    pub user_agent: Option<&'a str>,
    pub timeout_ms: Option<u64>,
    pub max_bytes: Option<u64>,
}

const MAX_REDIRECTS: usize = 10;
const DEFAULT_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// A GET with the session's cookies. Redirects are followed here, not by
/// reqwest, so every hop gets its own host's cookies. Returns
/// `{ url, status, contentType }` framed ahead of the body, like
/// `http::request_bytes`; any status is `Ok`.
pub async fn fetch(request: SessionFetch<'_>) -> Result<Vec<u8>, HttpError> {
    let invalid = |e: BrowserError| match e {
        BrowserError::InvalidArgument(m) => HttpError::InvalidUrl(m),
        other => HttpError::Other(format!("{other:?}")),
    };
    let mut url = web_url(request.url).map_err(invalid)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(request.timeout_ms.map_or(DEFAULT_FETCH_TIMEOUT, Duration::from_millis))
        .build()
        .map_err(|e| HttpError::Other(e.to_string()))?;
    for _ in 0..=MAX_REDIRECTS {
        let mut builder = client.get(url.clone());
        if let Some(cookies) = cookie_header(request.cookies, &url) {
            builder = builder.header(reqwest::header::COOKIE, cookies);
        }
        if let Some(user_agent) = request.user_agent {
            builder = builder.header(reqwest::header::USER_AGENT, user_agent);
        }
        let response = builder.send().await.map_err(http::send_error)?;
        let location = response.headers().get(reqwest::header::LOCATION).and_then(|l| l.to_str().ok());
        if let (true, Some(location)) = (response.status().is_redirection(), location) {
            let next = url.join(location).map_err(|e| HttpError::Other(format!("bad redirect {location:?}: {e}")))?;
            url = web_url(next.as_str()).map_err(invalid)?;
            continue;
        }
        let head = FetchHead {
            url: url.to_string(),
            status: response.status().as_u16(),
            content_type: response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned()),
        };
        return http::frame(&head, response, request.max_bytes).await;
    }
    Err(HttpError::Other(format!("more than {MAX_REDIRECTS} redirects")))
}

/// A rectangle in logical points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

const BESIDE_GAP: f64 = 12.0;

/// Where a `width` × `height` window goes next to the app window on
/// `screen`: right if it fits, else left. None when neither fits.
pub fn beside(app: Rect, width: f64, height: f64, screen: Rect) -> Option<(f64, f64)> {
    let lowest = (screen.y + screen.height - height).max(screen.y);
    let y = app.y.clamp(screen.y, lowest);
    let right = app.x + app.width + BESIDE_GAP;
    if right + width <= screen.x + screen.width {
        return Some((right, y));
    }
    let left = app.x - BESIDE_GAP - width;
    (left >= screen.x).then_some((left, y))
}

#[cfg(target_os = "macos")]
pub use macos::{can_go, clear_store, close_on_window_close, go, store_supported, title_bar_height};

#[cfg(target_os = "macos")]
mod macos {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::{sel, MainThreadMarker};
    use objc2_app_kit::NSWindow;
    use objc2_foundation::{NSDate, NSOperatingSystemVersion, NSProcessInfo, NSUUID};
    use objc2_web_kit::{WKWebView, WKWebsiteDataStore};

    /// `WKWebsiteDataStore(forIdentifier:)` is macOS 14+.
    pub fn store_supported() -> bool {
        let version = NSOperatingSystemVersion { majorVersion: 14, minorVersion: 0, patchVersion: 0 };
        NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(version)
    }

    /// Deletes everything in the session's store, whether or not a
    /// webview is using it; `done` runs on the main thread once it's gone.
    /// Call on the main thread.
    pub fn clear_store(key: [u8; 16], done: impl Fn() + 'static) -> Result<(), super::BrowserError> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| super::BrowserError::Other("a session store is cleared on the main thread".to_string()))?;
        unsafe {
            let identifier = NSUUID::from_bytes(key);
            let store = WKWebsiteDataStore::dataStoreForIdentifier(&identifier, mtm);
            let types = WKWebsiteDataStore::allWebsiteDataTypes(mtm);
            let block = block2::RcBlock::new(done);
            store.removeDataOfTypes_modifiedSince_completionHandler(&types, &NSDate::distantPast(), &block);
        }
        Ok(())
    }

    /// How far the title bar reaches into the window: a Tauri window's
    /// content view spans the whole frame, so its webviews start under it.
    /// Main thread.
    pub fn title_bar_height(ns_window: *mut std::ffi::c_void) -> f64 {
        // SAFETY: Tauri's Window::ns_window(), alive while the window is.
        let Some(window) = (unsafe { Retained::retain(ns_window.cast::<NSWindow>()) }) else { return 0.0 };
        let frame = window.frame();
        let content = window.contentLayoutRect();
        (frame.size.height - (content.origin.y + content.size.height)).max(0.0)
    }

    /// wry's UI delegate ignores WebKit's `webViewDidClose:`, so a sign-in
    /// popup's `window.close()` would leave it open. Adds the method to
    /// wry's delegate class, closing the window only for webviews on a
    /// Chain session store — never the app's own. Idempotent. Main thread.
    pub fn close_on_window_close(handle: *mut std::ffi::c_void) {
        let Some(delegate) = webview(handle).and_then(|webview| unsafe { webview.UIDelegate() }) else { return };
        unsafe {
            let class = objc2::ffi::object_getClass(Retained::as_ptr(&delegate).cast()) as *mut AnyClass;
            let did_close: unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut WKWebView) = did_close;
            // SAFETY: an Objective-C method taking (self, _cmd, WKWebView *), matching "v@:@".
            let imp = std::mem::transmute::<unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut WKWebView), Imp>(did_close);
            objc2::ffi::class_addMethod(class, sel!(webViewDidClose:), imp, c"v@:@".as_ptr());
        }
    }

    unsafe extern "C-unwind" fn did_close(_this: *mut AnyObject, _cmd: Sel, webview: *mut WKWebView) {
        let Some(webview) = (unsafe { Retained::retain(webview) }) else { return };
        let on_session_store = unsafe { webview.configuration().websiteDataStore().identifier() }.is_some();
        if let (true, Some(window)) = (on_session_store, webview.window()) {
            window.close();
        }
    }

    fn webview(handle: *mut std::ffi::c_void) -> Option<Retained<WKWebView>> {
        // SAFETY: Tauri's PlatformWebview::inner() is the page's WKWebView,
        // valid for the with_webview callback this runs in.
        unsafe { Retained::retain(handle.cast::<WKWebView>()) }
    }

    /// (can go back, can go forward). Main thread, from `with_webview`.
    pub fn can_go(handle: *mut std::ffi::c_void) -> (bool, bool) {
        webview(handle).map_or((false, false), |webview| unsafe { (webview.canGoBack(), webview.canGoForward()) })
    }

    /// Back when `back`, else forward. Main thread, from `with_webview`.
    pub fn go(handle: *mut std::ffi::c_void, back: bool) {
        if let Some(webview) = webview(handle) {
            unsafe {
                if back {
                    webview.goBack();
                } else {
                    webview.goForward();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie(name: &str, domain: &str, path: &str, secure: bool) -> SessionCookie {
        SessionCookie {
            name: name.to_string(),
            value: "v".to_string(),
            domain: Some(domain.to_string()),
            path: Some(path.to_string()),
            secure,
            expires_unix: None,
        }
    }

    fn url(s: &str) -> reqwest::Url {
        reqwest::Url::parse(s).unwrap()
    }

    #[test]
    fn session_names_default_and_are_bounded() {
        assert_eq!(session_name(None).unwrap(), "default");
        assert_eq!(session_name(Some("school b")).unwrap(), "school b");
        assert!(matches!(session_name(Some("")), Err(BrowserError::InvalidArgument(_))));
        assert!(matches!(session_name(Some(&"x".repeat(101))), Err(BrowserError::InvalidArgument(_))));
    }

    #[test]
    fn session_keys_are_stable_distinct_uuids() {
        let a = session_key("default");
        assert_eq!(a, session_key("default"));
        assert_ne!(a, session_key("other"));
        assert_eq!(a[6] >> 4, 8);
        assert_eq!(a[8] >> 6, 0b10);
        assert!(session_label("default").starts_with(LABEL_PREFIX));
        assert_eq!(session_label("default").len(), LABEL_PREFIX.len() + 32);
    }

    #[test]
    fn only_web_addresses_open() {
        assert!(web_url("https://canvas.school.edu/courses/1").is_ok());
        assert!(matches!(web_url("file:///etc/passwd"), Err(BrowserError::InvalidArgument(_))));
        assert!(matches!(web_url("not a url"), Err(BrowserError::InvalidArgument(_))));
    }

    #[test]
    fn the_page_never_reaches_the_app_origin() {
        let dev = url("http://localhost:1420/");
        assert!(may_navigate(&url("https://login.microsoftonline.com/"), Some(&dev)));
        assert!(may_navigate(&url("about:blank"), Some(&dev)));
        assert!(may_navigate(&url("blob:https://school.edu/1"), None));
        assert!(!may_navigate(&url("http://localhost:1420/settings"), Some(&dev)));
        assert!(!may_navigate(&url("http://tauri.localhost/"), None));
        assert!(!may_navigate(&url("tauri://localhost/"), None));
        assert!(!may_navigate(&url("chain-browser://localhost/action?do=button&id=import"), None));
        assert!(!may_navigate(&url("file:///Users/"), None));
    }

    #[test]
    fn buttons_need_unique_ids_and_labels() {
        let button = |id: &str, label: &str| Button { id: id.to_string(), label: label.to_string(), enabled: true };
        assert!(validate_buttons(&[button("import", "Import this page"), button("done", "Done")]).is_ok());
        assert!(matches!(validate_buttons(&[button("", "Done")]), Err(BrowserError::InvalidArgument(_))));
        assert!(matches!(validate_buttons(&[button("done", " ")]), Err(BrowserError::InvalidArgument(_))));
        assert!(matches!(
            validate_buttons(&[button("done", "Done"), button("done", "Close")]),
            Err(BrowserError::InvalidArgument(m)) if m.contains("twice")
        ));
    }

    #[test]
    fn dimensions_default_and_must_be_positive() {
        assert_eq!(dimension("width", None, 1100.0).unwrap(), 1100.0);
        assert_eq!(dimension("width", Some(640.0), 1100.0).unwrap(), 640.0);
        assert!(dimension("width", Some(0.0), 1100.0).is_err());
        assert!(dimension("width", Some(f64::NAN), 1100.0).is_err());
    }

    #[test]
    fn reads_page_content_and_its_failures() {
        let ready = r#"{"state":"ready","url":"https://s.edu/p","title":"Lesson","html":"<html></html>",
            "frames":[{"url":"https://s.edu/f","html":"<html>f</html>"}],"unreadableFrames":["https://video.example/"]}"#;
        let content = page_content(ready).unwrap();
        assert_eq!(content.title, "Lesson");
        assert_eq!(content.frames[0].url, "https://s.edu/f");
        assert_eq!(content.unreadable_frames, ["https://video.example/"]);
        assert!(matches!(page_content(r#"{"state":"loading"}"#), Err(BrowserError::Unavailable(_))));
        assert!(matches!(
            page_content(r#"{"state":"unsupported","contentType":"application/pdf"}"#),
            Err(BrowserError::Unsupported(m)) if m.contains("application/pdf")
        ));
        assert!(matches!(page_content(""), Err(BrowserError::Other(_))));
    }

    #[test]
    fn cookies_match_like_a_browser() {
        let cookies = vec![
            cookie("site", "school.edu", "/", false),
            cookie("canvas", "canvas.school.edu", "/", true),
            cookie("files", "canvas.school.edu", "/files", false),
            cookie("other", "evil.edu", "/", false),
            cookie("lookalike", "ool.edu", "/", false),
        ];
        assert_eq!(
            cookie_header(&cookies, &url("https://canvas.school.edu/files/1")).unwrap(),
            "files=v; site=v; canvas=v"
        );
        assert_eq!(cookie_header(&cookies, &url("http://canvas.school.edu/")).unwrap(), "site=v");
        assert_eq!(cookie_header(&cookies, &url("https://canvas.school.edu/filesystem")).unwrap(), "site=v; canvas=v");
        assert!(cookie_header(&cookies, &url("https://example.com/")).is_none());
    }

    #[test]
    fn expired_cookies_are_left_out() {
        let mut expired = cookie("old", "school.edu", "/", false);
        expired.expires_unix = Some(1);
        let mut fresh = cookie("new", "school.edu", "/", false);
        fresh.expires_unix = Some(i64::MAX);
        assert_eq!(cookie_header(&[expired, fresh], &url("https://school.edu/")).unwrap(), "new=v");
    }

    #[test]
    fn opens_beside_the_app_where_there_is_room() {
        let screen = Rect { x: 0.0, y: 25.0, width: 2000.0, height: 1100.0 };
        let app = Rect { x: 100.0, y: 100.0, width: 800.0, height: 600.0 };
        assert_eq!(beside(app, 1000.0, 800.0, screen), Some((912.0, 100.0)));
        let app_right = Rect { x: 1100.0, ..app };
        assert_eq!(beside(app_right, 1000.0, 800.0, screen), Some((88.0, 100.0)));
        let full = Rect { x: 0.0, y: 25.0, width: 2000.0, height: 1100.0 };
        assert_eq!(beside(full, 1000.0, 800.0, screen), None);
        let low = Rect { y: 900.0, ..app };
        assert_eq!(beside(low, 1000.0, 800.0, screen), Some((912.0, 325.0)));
    }

    mod fetch {
        use super::*;
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        /// Answers each connection in turn with the given responses and
        /// hands back each raw request.
        fn serve(responses: Vec<String>) -> (String, mpsc::Receiver<String>) {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) = listener.accept().unwrap();
                    let mut buf = [0u8; 4096];
                    let n = stream.read(&mut buf).unwrap();
                    tx.send(String::from_utf8_lossy(&buf[..n]).to_string()).unwrap();
                    stream.write_all(response.as_bytes()).unwrap();
                }
            });
            (format!("http://{addr}"), rx)
        }

        fn unframe(framed: &[u8]) -> (serde_json::Value, &[u8]) {
            let length = u32::from_be_bytes(framed[..4].try_into().unwrap()) as usize;
            (serde_json::from_slice(&framed[4..4 + length]).unwrap(), &framed[4 + length..])
        }

        #[tokio::test]
        async fn sends_each_hop_its_own_cookies_and_the_user_agent() {
            let (base, requests) = serve(vec![
                "HTTP/1.1 302 Found\r\nLocation: /image.png\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: 3\r\nConnection: close\r\n\r\nPNG".to_string(),
            ]);
            let cookies = vec![cookie("session", "127.0.0.1", "/", false), cookie("img", "127.0.0.1", "/image.png", false)];
            let framed = fetch(SessionFetch {
                url: &format!("{base}/page"),
                cookies: &cookies,
                user_agent: Some("TestAgent/1"),
                timeout_ms: Some(5_000),
                max_bytes: None,
            })
            .await
            .unwrap();
            let first = requests.recv().unwrap().to_ascii_lowercase();
            assert!(first.contains("cookie: session=v\r\n"), "{first}");
            assert!(first.contains("user-agent: testagent/1"), "{first}");
            let second = requests.recv().unwrap().to_ascii_lowercase();
            assert!(second.starts_with("get /image.png"), "{second}");
            assert!(second.contains("cookie: img=v; session=v\r\n"), "{second}");
            let (head, body) = unframe(&framed);
            assert_eq!(head["status"], 200);
            assert_eq!(head["contentType"], "image/png");
            assert_eq!(head["url"], format!("{base}/image.png"));
            assert_eq!(body, b"PNG");
        }

        #[tokio::test]
        async fn an_error_status_still_resolves_with_its_head() {
            let (base, _requests) = serve(vec![
                "HTTP/1.1 403 Forbidden\r\nContent-Length: 2\r\nConnection: close\r\n\r\nno".to_string(),
            ]);
            let framed = fetch(SessionFetch { url: &base, cookies: &[], user_agent: None, timeout_ms: None, max_bytes: None })
                .await
                .unwrap();
            let (head, body) = unframe(&framed);
            assert_eq!(head["status"], 403);
            assert!(head["contentType"].is_null());
            assert_eq!(body, b"no");
        }

        #[tokio::test]
        async fn refuses_non_web_addresses() {
            let result = fetch(SessionFetch { url: "file:///etc/hosts", cookies: &[], user_agent: None, timeout_ms: None, max_bytes: None }).await;
            assert!(matches!(result, Err(HttpError::InvalidUrl(_))));
        }
    }
}
