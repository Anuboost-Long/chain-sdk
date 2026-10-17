//! BrowserView capability — see /agent-docs/capabilities/browser-view/CONTRACT.md.
//! Other sites shown inside the app's own window. A view is a web view of
//! the OS's own, added above the app's page — never a Tauri webview, which
//! would get the app's IPC and stop Tauri treating the window as a webview
//! window. The Tauri layer (packages/cli/templates/browser_view.rs) only
//! routes commands and events; this module owns what a view may show, where
//! it sits, why a load failed, who started a navigation, and on macOS the
//! WebKit delegates that hold navigations.

use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum BrowserViewError {
    InvalidArgument(String),
    NotFound(String),
    Unsupported(String),
    Other(String),
}

/// How long a held navigation waits for an answer, and a window opened
/// with no address for its first one.
pub const HOLD_TIMEOUT: Duration = Duration::from_secs(10);

/// Whether a view may show `url`: web addresses, never the app's own
/// origin — its dev server, or a `*.localhost` custom-scheme host — since
/// those may call the app's commands. Other localhost ports are previews.
pub fn may_show(url: &reqwest::Url, app_origin: Option<&reqwest::Url>) -> bool {
    let app = app_origin.is_some_and(|app| app.origin() == url.origin());
    let custom_scheme_host = url.host_str().is_some_and(|host| host.ends_with(".localhost"));
    match url.scheme() {
        "about" | "data" | "blob" => !app,
        "http" | "https" => !app && !custom_scheme_host,
        _ => false,
    }
}

/// An address the app may load into a view.
pub fn view_url(url: &str, app_origin: Option<&reqwest::Url>) -> Result<reqwest::Url, BrowserViewError> {
    let parsed = reqwest::Url::parse(url).map_err(|e| BrowserViewError::InvalidArgument(format!("invalid URL {url:?}: {e}")))?;
    if !may_show(&parsed, app_origin) {
        return Err(BrowserViewError::InvalidArgument(format!("a view can't show {url:?}")));
    }
    Ok(parsed)
}

/// In the app page's CSS pixels, relative to its viewport.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Bounds {
    pub fn checked(self) -> Result<Self, BrowserViewError> {
        let finite = [self.x, self.y, self.width, self.height].iter().all(|v| v.is_finite());
        if !finite || self.width < 0.0 || self.height < 0.0 {
            return Err(BrowserViewError::InvalidArgument(format!(
                "bounds must be finite with a width and height of 0 or more, got {self:?}"
            )));
        }
        Ok(self)
    }

    /// Where these page bounds sit in the window, in logical points, for a
    /// page at `page_x`, `page_y` in its window, zoomed by `zoom`.
    pub fn in_window(self, page_x: f64, page_y: f64, zoom: f64) -> Self {
        Self { x: page_x + self.x * zoom, y: page_y + self.y * zoom, width: self.width * zoom, height: self.height * zoom }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureReason {
    Offline,
    HostNotFound,
    Unreachable,
    InsecureConnection,
    Blocked,
    Other,
}

/// Why a load failed, from the engine's error domain and code. None when
/// it was interrupted rather than failed — stopped, superseded, refused,
/// or turned into a download.
pub fn failure_reason(domain: &str, code: i64) -> Option<FailureReason> {
    use FailureReason::*;
    match domain {
        // NSURLErrorDomain: NSURLError.h.
        "NSURLErrorDomain" => Some(match code {
            -999 => return None,
            -1009 | -1018 | -1020 => Offline,
            -1003 | -1006 => HostNotFound,
            -1001 | -1004 | -1005 => Unreachable,
            -1206..=-1200 => InsecureConnection,
            -1022 => Blocked,
            _ => Other,
        }),
        // WebKitErrorDomain: WKError.h / WebKitErrors.h.
        "WebKitErrorDomain" => Some(match code {
            102 | 204 => return None,
            // A restricted port; a rule (setRules) blocked the page.
            103 | 104 => Blocked,
            _ => Other,
        }),
        _ => Some(Other),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NavigationKind {
    Link,
    Form,
    History,
    Reload,
    Other,
}

/// From WebKit's `WKNavigationType`.
pub fn navigation_kind(navigation_type: isize) -> NavigationKind {
    match navigation_type {
        0 => NavigationKind::Link,
        1 | 4 => NavigationKind::Form,
        2 => NavigationKind::History,
        3 => NavigationKind::Reload,
        _ => NavigationKind::Other,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Primary,
    Middle,
    Secondary,
}

/// From `WKNavigationAction.buttonNumber`, a button mask: 0 when no click started it.
pub fn mouse_button(button_number: isize) -> Option<MouseButton> {
    match button_number {
        1 => Some(MouseButton::Primary),
        2 => Some(MouseButton::Secondary),
        4 => Some(MouseButton::Middle),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
}

/// From AppKit's `NSEventModifierFlags`.
pub fn modifiers(flags: usize) -> Modifiers {
    Modifiers { shift: flags & 1 << 17 != 0, control: flags & 1 << 18 != 0, alt: flags & 1 << 19 != 0, meta: flags & 1 << 20 != 0 }
}

/// Whether going from `current` to `destination` stays in the same
/// document: only the fragment differs, and the destination has one.
pub fn same_document(current: &str, destination: &str) -> bool {
    let (Ok(current), Ok(destination)) = (reqwest::Url::parse(current), reqwest::Url::parse(destination)) else { return false };
    let without_fragment = |url: &reqwest::Url| url.as_str().split('#').next().unwrap_or_default().to_string();
    destination.fragment().is_some() && without_fragment(&current) == without_fragment(&destination)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub id: String,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadFailure {
    pub id: String,
    pub url: String,
    pub reason: FailureReason,
    pub description: String,
    pub native_code: i64,
    pub native_domain: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Crash {
    pub id: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Initiator {
    pub source_url: String,
    pub from_main_frame: bool,
    pub modifiers: Modifiers,
    pub button: Option<MouseButton>,
    pub input_age_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigationRequest {
    pub id: String,
    pub request_id: String,
    pub url: String,
    pub kind: NavigationKind,
    #[serde(flatten)]
    pub initiator: Initiator,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct WindowFeatures {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resizable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolbar: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub menubar: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewWindow {
    pub id: String,
    pub url: String,
    pub features: WindowFeatures,
    #[serde(flatten)]
    pub initiator: Initiator,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceType {
    Document,
    Image,
    StyleSheet,
    Script,
    Font,
    Media,
    SvgDocument,
    Popup,
    Ping,
    Fetch,
    Websocket,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoadType {
    FirstParty,
    ThirdParty,
}

/// A rule's trigger as the app writes it (camelCase) and WebKit reads it (kebab-case).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all(deserialize = "camelCase", serialize = "kebab-case"))]
pub struct RuleTrigger {
    pub url_filter: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_filter_is_case_sensitive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<Vec<ResourceType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_type: Option<Vec<LoadType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub if_domain: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unless_domain: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub if_top_url: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unless_top_url: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RuleAction {
    Block,
    BlockCookies,
    CssDisplayNone { selector: String },
    IgnorePreviousRules,
    MakeHttps,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Rule {
    pub trigger: RuleTrigger,
    pub action: RuleAction,
}

/// Reads rules one by one, so a bad one is named by its index.
pub fn parse_rules(rules: Vec<serde_json::Value>) -> Result<Vec<Rule>, BrowserViewError> {
    rules
        .into_iter()
        .enumerate()
        .map(|(i, rule)| serde_json::from_value(rule).map_err(|e| BrowserViewError::InvalidArgument(format!("rule {i}: {e}"))))
        .collect()
}

/// The first of `|` or `{` outside a character class, which the engine's
/// regular expressions don't support — caught here so the error names the rule.
fn unsupported_regex_part(pattern: &str) -> Option<char> {
    let (mut escaped, mut in_class) = (false, false);
    for c in pattern.chars() {
        match (escaped, c) {
            (true, _) => escaped = false,
            (false, '\\') => escaped = true,
            (false, '[') => in_class = true,
            (false, ']') => in_class = false,
            (false, '|' | '{') if !in_class => return Some(c),
            _ => {}
        }
    }
    None
}

/// The engine's limit for one compiled list.
pub const MAX_RULES: usize = 150_000;

/// Checks Chain's subset and writes the list as WebKit's content-blocker JSON.
pub fn content_rules_json(rules: &[Rule]) -> Result<String, BrowserViewError> {
    let invalid = |i: usize, m: &str| Err(BrowserViewError::InvalidArgument(format!("rule {i}: {m}")));
    if rules.is_empty() {
        return Err(BrowserViewError::InvalidArgument("a rule list needs at least one rule; removeRules() takes one away".to_string()));
    }
    if rules.len() > MAX_RULES {
        return Err(BrowserViewError::InvalidArgument(format!(
            "{} rules is more than one list holds ({MAX_RULES}); split them into several lists",
            rules.len()
        )));
    }
    for (i, Rule { trigger, action }) in rules.iter().enumerate() {
        if trigger.url_filter.is_empty() {
            return invalid(i, "urlFilter is empty");
        }
        if let Some(part) = unsupported_regex_part(&trigger.url_filter) {
            return invalid(i, &format!("urlFilter can't use `{part}` (CONTRACT.md lists the supported regular expressions)"));
        }
        let scopes = [&trigger.if_domain, &trigger.unless_domain, &trigger.if_top_url, &trigger.unless_top_url];
        if scopes.iter().filter(|scope| scope.is_some()).count() > 1 {
            return invalid(i, "use at most one of ifDomain, unlessDomain, ifTopUrl and unlessTopUrl");
        }
        if scopes.iter().any(|scope| scope.as_ref().is_some_and(|list| list.is_empty() || list.iter().any(String::is_empty))) {
            return invalid(i, "a domain or top-URL list can't be empty or hold an empty entry");
        }
        if matches!(action, RuleAction::CssDisplayNone { selector } if selector.trim().is_empty()) {
            return invalid(i, "css-display-none needs a selector");
        }
    }
    serde_json::to_string(rules).map_err(|e| BrowserViewError::Other(e.to_string()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The compiled list's name in the engine's store: the session and list
/// id (its prefix, so a replaced version can be found and deleted) and
/// the rules' own hash, so unchanged rules reuse what's compiled.
pub fn rule_list_identifier(session_key: [u8; 16], id: &str, rules_json: &str) -> (String, String) {
    use sha2::{Digest, Sha256};
    let prefix = format!("chain-view-{}-{}-", hex(&session_key), &hex(&Sha256::digest(id.as_bytes()))[..16]);
    let identifier = format!("{prefix}{}", &hex(&Sha256::digest(rules_json.as_bytes()))[..32]);
    (prefix, identifier)
}

/// The page-world half of `chainBrowserView.ask()`: JSON both ways, so
/// any JSON value crosses and the native side only ever sees strings.
pub const ASK_SCRIPT: &str = r#"(() => {
  const handler = window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.chainBrowserView;
  if (!handler || window.chainBrowserView) return;
  Object.defineProperty(window, "chainBrowserView", {
    value: Object.freeze({
      ask: (message) => handler.postMessage(JSON.stringify(message === undefined ? null : message)).then((reply) => JSON.parse(reply))
    })
  });
})();"#;

/// Runs in Chain's own world in every frame, so each frame reports in
/// (WebKit can't list them) and runInFrames() can reach it.
pub const FRAME_SCRIPT: &str = "window.webkit.messageHandlers.chainFrame.postMessage(null);";

/// An app's runInFrames() body as an async function whose result crosses as JSON.
pub fn frame_body(body: &str) -> String {
    format!("const value = await (async () => {{\n{body}\n}})();\nreturn JSON.stringify(value === undefined ? null : value);")
}

/// How long a page's ask() waits for the app's answer.
pub const ASK_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub url: String,
    pub main_frame: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub request_id: String,
    pub frame: Frame,
    pub message: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrameResult {
    pub frame: Frame,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What the native side reports, for the Tauri layer to emit. `Held*`
/// only start a timeout there; `Message` starts one too.
#[derive(Debug, Clone)]
pub enum Event {
    State(State),
    LoadFailed(LoadFailure),
    Crash(Crash),
    NavigationRequest(NavigationRequest),
    NewWindow(NewWindow),
    HeldWindow { held_id: String },
    Message(Message),
}

#[cfg(not(target_os = "macos"))]
pub use elsewhere::*;

/// Not built yet off macOS (see research/): every call is UNSUPPORTED.
#[cfg(not(target_os = "macos"))]
mod elsewhere {
    use super::{Bounds, BrowserViewError, Event, FrameResult, State};

    pub struct Create<'a> {
        pub page: *mut std::ffi::c_void,
        pub session_key: [u8; 16],
        pub session: &'a str,
        pub url: &'a str,
        pub bounds: Bounds,
        pub visible: bool,
        pub app_origin: Option<reqwest::Url>,
        pub decide_navigation: bool,
        pub allow_fullscreen: bool,
        pub allow_picture_in_picture: bool,
        pub swipe_navigation: bool,
        pub inspectable: bool,
    }

    fn unsupported<T>() -> Result<T, BrowserViewError> {
        Err(BrowserViewError::Unsupported("embedded views need macOS 14 or later today".to_string()))
    }

    pub fn supported() -> bool {
        false
    }
    pub fn picture_in_picture_supported() -> bool {
        false
    }
    pub fn set_sink(_sink: impl Fn(Event) + Send + Sync + 'static) {}
    pub fn create(_options: Create<'_>) -> Result<String, BrowserViewError> {
        unsupported()
    }
    pub fn set_bounds(_id: &str, _bounds: Bounds) -> Result<(), BrowserViewError> {
        unsupported()
    }
    pub fn set_visible(_id: &str, _visible: bool) -> Result<(), BrowserViewError> {
        unsupported()
    }
    pub fn close(_id: &str) {}
    pub fn navigate(_id: &str, _url: &str) -> Result<(), BrowserViewError> {
        unsupported()
    }
    pub fn go(_id: &str, _back: bool) -> Result<(), BrowserViewError> {
        unsupported()
    }
    pub fn reload(_id: &str) -> Result<(), BrowserViewError> {
        unsupported()
    }
    pub fn stop(_id: &str) -> Result<(), BrowserViewError> {
        unsupported()
    }
    pub fn state(_id: &str) -> Result<State, BrowserViewError> {
        unsupported()
    }
    pub fn answer(_request_id: &str, _allow: bool) {}
    pub fn drop_held(_held_id: &str) {}
    pub fn set_rules(
        _session: &str,
        _session_key: [u8; 16],
        _id: &str,
        _rules_json: String,
        done: impl FnOnce(Result<(), BrowserViewError>) + 'static,
    ) {
        done(unsupported());
    }
    pub fn remove_rules(_session: &str, _session_key: [u8; 16], _id: &str) {}
    pub fn set_script(_session: &str, _id: &str, _source: &str) {}
    pub fn remove_script(_session: &str, _id: &str) {}
    pub fn reply(_request_id: &str, _answer: Result<String, String>) {}
    pub fn run_in_frames(_id: &str, _body: &str, done: impl FnOnce(Result<Vec<FrameResult>, BrowserViewError>) + 'static) {
        done(unsupported());
    }
}

#[cfg(target_os = "macos")]
pub use macos::{
    answer, close, create, drop_held, go, navigate, reload, remove_rules, remove_script, reply, run_in_frames, set_bounds, set_rules, set_script,
    picture_in_picture_supported, set_sink, set_visible, state, stop, supported, Create,
};

#[cfg(target_os = "macos")]
mod macos {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::ptr::{null, null_mut, NonNull};
    use std::sync::OnceLock;
    use std::time::Instant;

    use block2::{DynBlock, RcBlock};
    use objc2::rc::{Retained, Weak};
    use objc2::runtime::{AnyObject, NSObject, ProtocolObject, Sel};
    use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly, Message as _};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSEvent, NSEventMask, NSEventType, NSMenu, NSModalResponseOK, NSOpenPanel, NSResponder, NSUserInterfaceItemIdentification, NSView,
        NSWindowOrderingMode,
    };
    use objc2_foundation::{
        ns_string, NSArray, NSDictionary, NSError, NSKeyValueChangeKey, NSKeyValueObservingOptions,
        NSObjectNSKeyValueObserverRegistration, NSObjectProtocol, NSOperatingSystemVersion, NSPoint, NSProcessInfo, NSRect, NSSize, NSString,
        NSURLRequest, NSURL, NSUUID,
    };
    use objc2_web_kit::{
        WKFrameInfo, WKMediaCaptureType, WKNavigation, WKNavigationAction, WKNavigationActionPolicy, WKNavigationDelegate,
        WKContentRuleList, WKContentRuleListStore, WKContentWorld, WKErrorCode, WKOpenPanelParameters, WKPermissionDecision, WKScriptMessage,
        WKScriptMessageHandler, WKScriptMessageHandlerWithReply, WKSecurityOrigin, WKUIDelegate, WKUserContentController, WKUserScript,
        WKUserScriptInjectionTime, WKWebView, WKWebViewConfiguration, WKWebsiteDataStore, WKWindowFeatures,
    };

    use super::*;

    /// Named persistent stores (`WKWebsiteDataStore(forIdentifier:)`) are macOS 14+.
    pub fn supported() -> bool {
        let version = NSOperatingSystemVersion { majorVersion: 14, minorVersion: 0, patchVersion: 0 };
        NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(version)
    }

    type Sink = Box<dyn Fn(Event) + Send + Sync>;
    static SINK: OnceLock<Sink> = OnceLock::new();

    /// Where every view's events go. The first call wins.
    pub fn set_sink(sink: impl Fn(Event) + Send + Sync + 'static) {
        let _ = SINK.set(Box::new(sink));
    }

    fn emit(event: Event) {
        if let Some(sink) = SINK.get() {
            sink(event);
        }
    }

    struct View {
        webview: Retained<WKWebView>,
        page: Weak<WKWebView>,
        delegate: Retained<ViewDelegate>,
        decide: bool,
        last_input: Option<Instant>,
        // The next main-frame navigation is the app's own load/back/forward/reload.
        app_navigation: bool,
        // An allowed main-frame navigation hasn't committed yet: what tells
        // its server redirects apart where WebKit doesn't flag them.
        provisional: bool,
        reported: Option<State>,
        // What reload() restarts once the page process is gone and WebKit forgot it.
        last_url: Option<String>,
        session: String,
        content: Retained<WKUserContentController>,
        // Every frame that reported in since the page's document started (FRAME_SCRIPT).
        frames: Vec<Retained<WKFrameInfo>>,
    }

    /// A session's rules and scripts, put on each of its views.
    #[derive(Default)]
    struct SessionContent {
        rules: Vec<(String, Retained<WKContentRuleList>)>,
        scripts: Vec<(String, String)>,
    }

    struct Reply {
        view: String,
        reply: RcBlock<dyn Fn(*mut AnyObject, *mut NSString)>,
    }

    struct Held {
        opener: String,
        webview: Retained<WKWebView>,
        _delegate: Retained<ViewDelegate>,
        initiator: Initiator,
        features: WindowFeatures,
        reported: bool,
    }

    struct Pending {
        view: String,
        decide: RcBlock<dyn Fn(WKNavigationActionPolicy)>,
    }

    #[derive(Default)]
    struct Registry {
        views: HashMap<String, View>,
        pending: HashMap<String, Pending>,
        held: HashMap<String, Held>,
        next: u64,
        app_origin: Option<reqwest::Url>,
        input_monitor: Option<Retained<AnyObject>>,
        sessions: HashMap<String, SessionContent>,
        replies: HashMap<String, Reply>,
    }

    impl Registry {
        fn next_id(&mut self) -> String {
            self.next += 1;
            self.next.to_string()
        }
    }

    thread_local! {
        // Main thread only: WebKit's objects and decision blocks never leave it.
        static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
    }

    // Never call into WebKit inside this: it may call a delegate back synchronously.
    fn with<R>(f: impl FnOnce(&mut Registry) -> R) -> R {
        REGISTRY.with(|registry| f(&mut registry.borrow_mut()))
    }

    enum Owner {
        View(String),
        Held(String),
    }

    struct Ivars {
        owner: Owner,
    }

    const OBSERVED: [&str; 5] = ["URL", "title", "loading", "canGoBack", "canGoForward"];

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[ivars = Ivars]
        struct ViewDelegate;

        unsafe impl NSObjectProtocol for ViewDelegate {}

        unsafe impl WKNavigationDelegate for ViewDelegate {
            #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
            fn decide_policy(
                &self,
                webview: &WKWebView,
                action: &WKNavigationAction,
                handler: &DynBlock<dyn Fn(WKNavigationActionPolicy)>,
            ) {
                decide_policy(&self.ivars().owner, webview, action, handler);
            }

            #[unsafe(method(webView:didCommitNavigation:))]
            fn did_commit(&self, _webview: &WKWebView, _navigation: Option<&WKNavigation>) {
                self.settled();
            }

            #[unsafe(method(webView:didFinishNavigation:))]
            fn did_finish(&self, _webview: &WKWebView, _navigation: Option<&WKNavigation>) {
                self.settled();
            }

            #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
            fn did_fail_provisional(&self, webview: &WKWebView, _navigation: Option<&WKNavigation>, error: &NSError) {
                self.failed(webview, error);
            }

            #[unsafe(method(webView:didFailNavigation:withError:))]
            fn did_fail(&self, webview: &WKWebView, _navigation: Option<&WKNavigation>, error: &NSError) {
                self.failed(webview, error);
            }

            #[unsafe(method(webViewWebContentProcessDidTerminate:))]
            fn crashed(&self, webview: &WKWebView) {
                let Owner::View(id) = &self.ivars().owner else { return };
                self.settled();
                emit(Event::Crash(Crash { id: id.clone(), url: webview_url(webview) }));
                emit_state(id);
            }
        }

        // FRAME_SCRIPT, from Chain's own world: a frame's document started.
        unsafe impl WKScriptMessageHandler for ViewDelegate {
            #[unsafe(method(userContentController:didReceiveScriptMessage:))]
            fn frame_started(&self, _controller: &WKUserContentController, message: &WKScriptMessage) {
                if let Owner::View(id) = &self.ivars().owner {
                    record_frame(id, unsafe { message.frameInfo() });
                }
            }
        }

        // chainBrowserView.ask(), from the page's world: untrusted, but the
        // frame it names is the engine's.
        unsafe impl WKScriptMessageHandlerWithReply for ViewDelegate {
            #[unsafe(method(userContentController:didReceiveScriptMessage:replyHandler:))]
            fn asked(
                &self,
                _controller: &WKUserContentController,
                message: &WKScriptMessage,
                reply: &DynBlock<dyn Fn(*mut AnyObject, *mut NSString)>,
            ) {
                match &self.ivars().owner {
                    Owner::View(id) => asked(id, message, reply),
                    Owner::Held(_) => reply_with(reply, Err("this page has no view".to_string())),
                }
            }
        }

        unsafe impl WKUIDelegate for ViewDelegate {
            #[unsafe(method_id(webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:))]
            fn create_web_view(
                &self,
                _webview: &WKWebView,
                configuration: &WKWebViewConfiguration,
                action: &WKNavigationAction,
                features: &WKWindowFeatures,
            ) -> Option<Retained<WKWebView>> {
                match &self.ivars().owner {
                    Owner::View(id) => new_window(id, configuration, action, features),
                    Owner::Held(_) => None,
                }
            }

            #[unsafe(method(webView:requestMediaCapturePermissionForOrigin:initiatedByFrame:type:decisionHandler:))]
            fn media_capture(
                &self,
                _webview: &WKWebView,
                _origin: &WKSecurityOrigin,
                _frame: &WKFrameInfo,
                _capture: WKMediaCaptureType,
                handler: &DynBlock<dyn Fn(WKPermissionDecision)>,
            ) {
                handler.call((WKPermissionDecision::Deny,));
            }

            #[unsafe(method(webView:runOpenPanelWithParameters:initiatedByFrame:completionHandler:))]
            fn open_panel(
                &self,
                _webview: &WKWebView,
                parameters: &WKOpenPanelParameters,
                _frame: &WKFrameInfo,
                handler: &DynBlock<dyn Fn(*const NSArray<NSURL>)>,
            ) {
                let Some(mtm) = MainThreadMarker::new() else { return handler.call((null(),)) };
                let panel = NSOpenPanel::openPanel(mtm);
                panel.setCanChooseFiles(true);
                unsafe {
                    panel.setAllowsMultipleSelection(parameters.allowsMultipleSelection());
                    panel.setCanChooseDirectories(parameters.allowsDirectories());
                }
                if panel.runModal() == NSModalResponseOK {
                    handler.call((Retained::as_ptr(&panel.URLs()),));
                } else {
                    handler.call((null(),));
                }
            }
        }

        impl ViewDelegate {
            // Not in objc2-web-kit's bindings, but a real WKUIDelegate method.
            #[unsafe(method(webView:requestGeolocationPermissionForOrigin:initiatedByFrame:decisionHandler:))]
            fn geolocation(
                &self,
                _webview: &WKWebView,
                _origin: &AnyObject,
                _frame: &AnyObject,
                handler: &DynBlock<dyn Fn(WKPermissionDecision)>,
            ) {
                handler.call((WKPermissionDecision::Deny,));
            }

            #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
            fn observe(
                &self,
                _key_path: Option<&NSString>,
                _object: Option<&AnyObject>,
                _change: Option<&NSDictionary<NSKeyValueChangeKey, AnyObject>>,
                _context: *mut c_void,
            ) {
                if let Owner::View(id) = &self.ivars().owner {
                    emit_state(id);
                }
            }
        }
    );

    define_class!(
        #[unsafe(super(WKWebView, NSView, NSResponder, NSObject))]
        #[thread_kind = MainThreadOnly]
        struct ChainWebView;

        impl ChainWebView {
            #[unsafe(method(willOpenMenu:withEvent:))]
            fn will_open_menu(&self, menu: &NSMenu, event: &NSEvent) {
                let _: () = unsafe { msg_send![super(self), willOpenMenu: menu, withEvent: event] };
                remove_new_window_and_download_items(menu);
            }
        }
    );

    // The engine's own menu would open a window or start a download, which
    // a view never does: those items are silently dead, so they go. The
    // identifiers are WebKit's own (WKMenuItemIdentifiers.h), whose values
    // are their names.
    const NEW_WINDOW_AND_DOWNLOAD_ITEMS: [&str; 7] = [
        "WKMenuItemIdentifierOpenLinkInNewWindow",
        "WKMenuItemIdentifierDownloadLinkedFile",
        "WKMenuItemIdentifierOpenImageInNewWindow",
        "WKMenuItemIdentifierDownloadImage",
        "WKMenuItemIdentifierOpenFrameInNewWindow",
        "WKMenuItemIdentifierOpenMediaInNewWindow",
        "WKMenuItemIdentifierDownloadMedia",
    ];

    fn remove_new_window_and_download_items(menu: &NSMenu) {
        for item in menu.itemArray() {
            if item.identifier().is_some_and(|id| NEW_WINDOW_AND_DOWNLOAD_ITEMS.contains(&id.to_string().as_str())) {
                menu.removeItem(&item);
            }
        }
    }

    impl ViewDelegate {
        fn new(mtm: MainThreadMarker, owner: Owner) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(Ivars { owner });
            unsafe { msg_send![super(this), init] }
        }

        fn settled(&self) {
            if let Owner::View(id) = &self.ivars().owner {
                with(|r| {
                    if let Some(view) = r.views.get_mut(id) {
                        view.provisional = false;
                        view.app_navigation = false;
                    }
                });
            }
        }

        fn failed(&self, webview: &WKWebView, error: &NSError) {
            let Owner::View(id) = &self.ivars().owner else { return };
            self.settled();
            let domain = error.domain().to_string();
            let code = error.code() as i64;
            let Some(reason) = failure_reason(&domain, code) else { return };
            emit(Event::LoadFailed(LoadFailure {
                id: id.clone(),
                url: failing_url(error).unwrap_or_else(|| webview_url(webview)),
                reason,
                description: error.localizedDescription().to_string(),
                native_code: code,
                native_domain: domain,
            }));
        }
    }

    fn failing_url(error: &NSError) -> Option<String> {
        let info = error.userInfo();
        if let Some(url) = info.objectForKey(ns_string!("NSErrorFailingURLKey")).and_then(|v| v.downcast::<NSURL>().ok()) {
            return url.absoluteString().map(|url| url.to_string());
        }
        let url = info.objectForKey(ns_string!("NSErrorFailingURLStringKey"))?;
        url.downcast::<NSString>().ok().map(|url| url.to_string())
    }

    fn webview_url(webview: &WKWebView) -> String {
        unsafe { webview.URL() }.and_then(|url| url.absoluteString()).map(|url| url.to_string()).unwrap_or_default()
    }

    fn request_url(request: &NSURLRequest) -> String {
        request.URL().and_then(|url| url.absoluteString()).map(|url| url.to_string()).unwrap_or_default()
    }

    fn read_state(id: &str, webview: &WKWebView) -> State {
        unsafe {
            State {
                id: id.to_string(),
                url: webview_url(webview),
                title: webview.title().map(|title| title.to_string()).unwrap_or_default(),
                loading: webview.isLoading(),
                can_go_back: webview.canGoBack(),
                can_go_forward: webview.canGoForward(),
            }
        }
    }

    fn emit_state(id: &str) {
        let Some(webview) = with(|r| r.views.get(id).map(|view| view.webview.clone())) else { return };
        let state = read_state(id, &webview);
        let changed = with(|r| {
            let Some(view) = r.views.get_mut(id) else { return false };
            let changed = view.reported.as_ref() != Some(&state);
            if !state.url.is_empty() {
                view.last_url = Some(state.url.clone());
            }
            view.reported = Some(state.clone());
            changed
        });
        if changed {
            emit(Event::State(state));
        }
    }

    fn may_show_str(url: &str, app_origin: Option<&reqwest::Url>) -> bool {
        reqwest::Url::parse(url).is_ok_and(|url| may_show(&url, app_origin))
    }

    fn initiator(action: &WKNavigationAction, last_input: Option<Instant>) -> Initiator {
        let source = unsafe { action.sourceFrame() };
        Initiator {
            source_url: source.as_ref().map(|frame| request_url(&*unsafe { frame.request() })).unwrap_or_default(),
            from_main_frame: source.as_ref().is_none_or(|frame| unsafe { frame.isMainFrame() }),
            modifiers: modifiers(unsafe { action.modifierFlags() }.0),
            button: mouse_button(unsafe { action.buttonNumber() }),
            input_age_ms: last_input.map(|at| at.elapsed().as_millis() as u64),
        }
    }

    fn decide_policy(owner: &Owner, webview: &WKWebView, action: &WKNavigationAction, handler: &DynBlock<dyn Fn(WKNavigationActionPolicy)>) {
        let url = request_url(&*unsafe { action.request() });
        let app_origin = with(|r| r.app_origin.clone());
        let allowed = may_show_str(&url, app_origin.as_ref());
        let id = match owner {
            Owner::Held(held_id) => return decide_held(held_id, &url, allowed, handler),
            Owner::View(id) => id,
        };
        let download = action.respondsToSelector(sel!(shouldPerformDownload)) && unsafe { action.shouldPerformDownload() };
        if download || !allowed {
            return handler.call((WKNavigationActionPolicy::Cancel,));
        }
        // No target frame: a new window, decided in createWebView.
        let main_frame = unsafe { action.targetFrame() }.is_some_and(|frame| unsafe { frame.isMainFrame() });
        if !main_frame {
            return handler.call((WKNavigationActionPolicy::Allow,));
        }
        let kind = navigation_kind(unsafe { action.navigationType() }.0);
        let in_page = same_document(&webview_url(webview), &url);
        // A redirect repeats its navigation's own type, so only WebKit's
        // (unpublished) flag tells it apart from a new attempt mid-load.
        let flagged_redirect = action
            .respondsToSelector(sel!(_isRedirect))
            .then(|| unsafe { msg_send![action, _isRedirect] });
        let held = with(|r| {
            let request_id = r.next_id();
            let view = r.views.get_mut(id)?;
            let redirect = flagged_redirect.unwrap_or(view.provisional);
            if std::mem::take(&mut view.app_navigation) || redirect || !view.decide || in_page {
                view.provisional = true;
                return None;
            }
            let last_input = view.last_input;
            r.pending.insert(request_id.clone(), Pending { view: id.clone(), decide: handler.copy() });
            Some((request_id, last_input))
        });
        let Some((request_id, last_input)) = held else {
            return handler.call((WKNavigationActionPolicy::Allow,));
        };
        emit(Event::NavigationRequest(NavigationRequest {
            id: id.clone(),
            request_id,
            url,
            kind,
            initiator: initiator(action, last_input),
        }));
    }

    // A window opened with no address: its first real one is reported as
    // the opener's new window, and it never loads anything itself.
    fn decide_held(held_id: &str, url: &str, allowed: bool, handler: &DynBlock<dyn Fn(WKNavigationActionPolicy)>) {
        if url.is_empty() || url == "about:blank" {
            return handler.call((WKNavigationActionPolicy::Allow,));
        }
        handler.call((WKNavigationActionPolicy::Cancel,));
        // Kept until drop_held: it's still inside its own callback.
        let report = with(|r| {
            let held = r.held.get_mut(held_id).filter(|held| !held.reported)?;
            held.reported = true;
            Some(NewWindow { id: held.opener.clone(), url: url.to_string(), features: held.features.clone(), initiator: held.initiator.clone() })
        });
        if let (Some(report), true) = (report, allowed) {
            emit(Event::NewWindow(report));
        }
    }

    fn window_features(features: &WKWindowFeatures) -> WindowFeatures {
        unsafe {
            let number = |n: Option<Retained<objc2_foundation::NSNumber>>| n.map(|n| n.doubleValue());
            let flag = |n: Option<Retained<objc2_foundation::NSNumber>>| n.map(|n| n.boolValue());
            WindowFeatures {
                left: number(features.x()),
                top: number(features.y()),
                width: number(features.width()),
                height: number(features.height()),
                resizable: flag(features.allowsResizing()),
                toolbar: flag(features.toolbarsVisibility()),
                menubar: flag(features.menuBarVisibility()),
                status: flag(features.statusBarVisibility()),
            }
        }
    }

    fn new_window(
        id: &str,
        configuration: &WKWebViewConfiguration,
        action: &WKNavigationAction,
        features: &WKWindowFeatures,
    ) -> Option<Retained<WKWebView>> {
        let url = request_url(&*unsafe { action.request() });
        let (last_input, app_origin) = with(|r| (r.views.get(id).and_then(|view| view.last_input), r.app_origin.clone()));
        let initiator = initiator(action, last_input);
        let features = window_features(features);
        if !url.is_empty() && url != "about:blank" {
            if may_show_str(&url, app_origin.as_ref()) {
                emit(Event::NewWindow(NewWindow { id: id.to_string(), url, features, initiator }));
            }
            return None;
        }
        // WebKit needs the window it returns built from `configuration`.
        let mtm = MainThreadMarker::new()?;
        let webview = unsafe { WKWebView::initWithFrame_configuration(WKWebView::alloc(mtm), NSRect::ZERO, configuration) };
        let held_id = with(|r| r.next_id());
        let delegate = ViewDelegate::new(mtm, Owner::Held(held_id.clone()));
        unsafe { webview.setNavigationDelegate(Some(ProtocolObject::from_ref(&*delegate))) };
        with(|r| {
            r.held.insert(
                held_id.clone(),
                Held { opener: id.to_string(), webview: webview.clone(), _delegate: delegate, initiator, features, reported: false },
            )
        });
        emit(Event::HeldWindow { held_id });
        Some(webview)
    }

    /// Forgets a window opened with no address, once its time is up.
    pub fn drop_held(held_id: &str) {
        let held = with(|r| r.held.remove(held_id));
        if let Some(held) = held {
            unsafe {
                held.webview.stopLoading();
                held.webview.setNavigationDelegate(None);
            }
        }
    }

    fn record_input(event: &NSEvent) {
        let key = event.r#type() == NSEventType::KeyDown;
        let Some(window) = event.window(MainThreadMarker::new().expect("AppKit events arrive on the main thread")) else { return };
        let responder = window.firstResponder();
        let point = event.locationInWindow();
        with(|r| {
            for view in r.views.values_mut() {
                let webview: &NSView = &view.webview;
                if webview.window().as_deref() != Some(&*window) || webview.isHiddenOrHasHiddenAncestor() {
                    continue;
                }
                let inside = if key {
                    responder.as_ref().and_then(|r| r.downcast_ref::<NSView>()).is_some_and(|focused| focused.isDescendantOf(webview))
                } else {
                    let local = webview.convertPoint_fromView(point, None);
                    let bounds = webview.bounds();
                    let (x, y) = (local.x - bounds.origin.x, local.y - bounds.origin.y);
                    (0.0..bounds.size.width).contains(&x) && (0.0..bounds.size.height).contains(&y)
                };
                if inside {
                    view.last_input = Some(Instant::now());
                }
            }
        });
    }

    fn install_input_monitor() {
        if with(|r| r.input_monitor.is_some()) {
            return;
        }
        let mask = NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown | NSEventMask::KeyDown;
        let block = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
            record_input(unsafe { event.as_ref() });
            event.as_ptr()
        });
        let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) };
        with(|r| r.input_monitor = monitor);
    }

    pub struct Create<'a> {
        /// The app page's WKWebView — Tauri's `PlatformWebview::inner()`, from `with_webview`.
        pub page: *mut c_void,
        /// `browser::session_key`, so a name is the same store as desktop.browser's.
        pub session_key: [u8; 16],
        /// Whose rules and scripts it gets (setRules/setScript).
        pub session: &'a str,
        pub url: &'a str,
        pub bounds: Bounds,
        pub visible: bool,
        pub app_origin: Option<reqwest::Url>,
        pub decide_navigation: bool,
        pub allow_fullscreen: bool,
        pub allow_picture_in_picture: bool,
        pub swipe_navigation: bool,
        pub inspectable: bool,
    }

    // macOS has no public switch for a page's requestPictureInPicture();
    // without this unpublished one it always fails NotSupportedError.
    fn picture_in_picture_selector() -> Sel {
        sel!(_setAllowsPictureInPictureMediaPlayback:)
    }

    pub fn picture_in_picture_supported() -> bool {
        supported() && unsafe { objc2::ffi::class_respondsToSelector(objc2::class!(WKPreferences), picture_in_picture_selector()).as_bool() }
    }

    /// A web view of its own above the app's page, on the session's store,
    /// loading `url`. It's never a Tauri webview: no IPC, no app scripts.
    /// Resolves with its id. Main thread.
    pub fn create(options: Create<'_>) -> Result<String, BrowserViewError> {
        let mtm = MainThreadMarker::new().ok_or_else(|| BrowserViewError::Other("views are created on the main thread".to_string()))?;
        // SAFETY: Tauri's PlatformWebview::inner() is the page's WKWebView, alive for the with_webview callback.
        let page = unsafe { Retained::retain(options.page.cast::<WKWebView>()) }
            .ok_or_else(|| BrowserViewError::Other("the app page has no web view".to_string()))?;
        let superview = unsafe { page.superview() }.ok_or_else(|| BrowserViewError::Other("the app page isn't in a window".to_string()))?;
        let id = with(|r| r.next_id());
        let delegate = ViewDelegate::new(mtm, Owner::View(id.clone()));
        let content = unsafe {
            let content = WKUserContentController::new(mtm);
            content.addScriptMessageHandlerWithReply_contentWorld_name(
                ProtocolObject::from_ref(&*delegate),
                &WKContentWorld::pageWorld(mtm),
                ns_string!("chainBrowserView"),
            );
            content.addScriptMessageHandler_contentWorld_name(ProtocolObject::from_ref(&*delegate), &chain_world(mtm), ns_string!("chainFrame"));
            content
        };
        let webview = unsafe {
            let configuration = WKWebViewConfiguration::new(mtm);
            configuration.setUserContentController(&content);
            configuration.setWebsiteDataStore(&WKWebsiteDataStore::dataStoreForIdentifier(&NSUUID::from_bytes(options.session_key), mtm));
            configuration.preferences().setElementFullscreenEnabled(options.allow_fullscreen);
            let preferences = configuration.preferences();
            if options.allow_picture_in_picture && preferences.respondsToSelector(picture_in_picture_selector()) {
                let _: () = msg_send![&*preferences, _setAllowsPictureInPictureMediaPlayback: true];
            }
            let webview: Retained<ChainWebView> =
                msg_send![super(ChainWebView::alloc(mtm).set_ivars(())), initWithFrame: NSRect::ZERO, configuration: &*configuration];
            Retained::into_super(webview)
        };
        unsafe {
            webview.setNavigationDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            webview.setUIDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            webview.setAllowsBackForwardNavigationGestures(options.swipe_navigation);
            if webview.respondsToSelector(sel!(setInspectable:)) {
                webview.setInspectable(options.inspectable);
            }
            for key in OBSERVED {
                webview.addObserver_forKeyPath_options_context(&delegate, &NSString::from_str(key), NSKeyValueObservingOptions::New, null_mut());
            }
        }
        webview.setHidden(!options.visible);
        // Keeps its distance from the top when the window resizes, until the app places it again.
        if !superview.isFlipped() {
            webview.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        }
        superview.addSubview_positioned_relativeTo(&webview, NSWindowOrderingMode::Above, None);
        place(&page, &webview, options.bounds);
        with(|r| {
            r.app_origin = options.app_origin;
            r.views.insert(
                id.clone(),
                View {
                    webview,
                    page: Weak::from_retained(&page),
                    delegate,
                    decide: options.decide_navigation,
                    last_input: None,
                    app_navigation: false,
                    provisional: false,
                    reported: None,
                    last_url: None,
                    session: options.session.to_string(),
                    content,
                    frames: Vec::new(),
                },
            );
        });
        put_session_content(options.session);
        install_input_monitor();
        navigate(&id, options.url)?;
        Ok(id)
    }

    // Page CSS pixels → the page's own points (page zoom) → its superview's.
    fn place(page: &WKWebView, webview: &WKWebView, bounds: Bounds) {
        let Some(superview) = (unsafe { page.superview() }) else { return };
        let zoom = unsafe { page.pageZoom() };
        let scaled = bounds.in_window(0.0, 0.0, zoom);
        let y = if page.isFlipped() { scaled.y } else { page.bounds().size.height - scaled.y - scaled.height };
        let rect = NSRect::new(NSPoint::new(scaled.x, y), NSSize::new(scaled.width, scaled.height));
        webview.setFrame(superview.convertRect_fromView(rect, Some(page)));
    }

    pub fn set_bounds(id: &str, bounds: Bounds) -> Result<(), BrowserViewError> {
        let (webview, page) = with(|r| r.views.get(id).map(|view| (view.webview.clone(), view.page.load())))
            .ok_or_else(|| not_found(id))?;
        if let Some(page) = page {
            place(&page, &webview, bounds);
        }
        Ok(())
    }

    pub fn set_visible(id: &str, visible: bool) -> Result<(), BrowserViewError> {
        webview(id)?.setHidden(!visible);
        Ok(())
    }

    /// Removes the view: no more events, held navigations refused. A no-op
    /// for an unknown id. Main thread.
    pub fn close(id: &str) {
        let Some((view, pending, held, replies)) = with(|r| {
            let view = r.views.remove(id)?;
            let pending: Vec<_> = r.pending.extract_if(|_, p| p.view == id).map(|(_, p)| p.decide).collect();
            let held: Vec<_> = r.held.extract_if(|_, h| h.opener == id).map(|(_, h)| h.webview).collect();
            let replies: Vec<_> = r.replies.extract_if(|_, reply| reply.view == id).map(|(_, reply)| reply.reply).collect();
            Some((view, pending, held, replies))
        }) else {
            return;
        };
        unsafe {
            for key in OBSERVED {
                view.webview.removeObserver_forKeyPath(&view.delegate, &NSString::from_str(key));
            }
            view.webview.stopLoading();
            view.webview.setNavigationDelegate(None);
            view.webview.setUIDelegate(None);
            // The controller holds its handlers strongly.
            view.content.removeAllScriptMessageHandlers();
            for webview in held {
                webview.stopLoading();
                webview.setNavigationDelegate(None);
            }
        }
        view.webview.removeFromSuperview();
        for decide in pending {
            decide.call((WKNavigationActionPolicy::Cancel,));
        }
        for reply in replies {
            reply_with(&reply, Err("the view closed".to_string()));
        }
    }

    fn chain_world(mtm: MainThreadMarker) -> Retained<WKContentWorld> {
        unsafe { WKContentWorld::worldWithName(ns_string!("chain"), mtm) }
    }

    fn frame_of(frame: &WKFrameInfo) -> Frame {
        unsafe { Frame { url: request_url(&frame.request()), main_frame: frame.isMainFrame() } }
    }

    // A main frame's report means a new page: its earlier frames are gone.
    fn record_frame(id: &str, frame: Retained<WKFrameInfo>) {
        let main = unsafe { frame.isMainFrame() };
        with(|r| {
            let Some(view) = r.views.get_mut(id) else { return };
            if main {
                view.frames.clear();
            } else if !view.frames.iter().any(|known| known.isEqual(Some(&frame))) {
                view.frames.push(frame);
            }
        });
    }

    fn reply_with(reply: &DynBlock<dyn Fn(*mut AnyObject, *mut NSString)>, answer: Result<String, String>) {
        match answer {
            Ok(json) => reply.call((Retained::as_ptr(&NSString::from_str(&json)) as *mut AnyObject, null_mut())),
            Err(why) => reply.call((null_mut(), Retained::as_ptr(&NSString::from_str(&why)) as *mut NSString)),
        }
    }

    fn asked(id: &str, message: &WKScriptMessage, reply: &DynBlock<dyn Fn(*mut AnyObject, *mut NSString)>) {
        let body = unsafe { message.body() }.downcast::<NSString>().ok().map(|body| body.to_string());
        let Some(message_json) = body.and_then(|body| serde_json::from_str(&body).ok()) else {
            return reply_with(reply, Err("ask() sends JSON".to_string()));
        };
        let frame = frame_of(&*unsafe { message.frameInfo() });
        let request_id = with(|r| {
            let request_id = r.next_id();
            r.replies.insert(request_id.clone(), Reply { view: id.to_string(), reply: reply.copy() });
            request_id
        });
        emit(Event::Message(Message { id: id.to_string(), request_id, frame, message: message_json }));
    }

    /// Answers a page's ask(): the app's answer as JSON, or why there's none.
    /// Nothing once it's answered, expired, or its view closed.
    pub fn reply(request_id: &str, answer: Result<String, String>) {
        if let Some(reply) = with(|r| r.replies.remove(request_id)) {
            reply_with(&reply.reply, answer);
        }
    }

    // Puts the session's current rules and scripts on each of its views;
    // the engine applies them from each one's next load.
    fn put_session_content(session: &str) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let (contents, rules, scripts) = with(|r| {
            let contents: Vec<_> = r.views.values().filter(|view| view.session == session).map(|view| view.content.clone()).collect();
            let current = r.sessions.get(session);
            let rules: Vec<_> = current.map(|c| c.rules.iter().map(|(_, list)| list.clone()).collect()).unwrap_or_default();
            let scripts: Vec<_> = current.map(|c| c.scripts.iter().map(|(_, source)| source.clone()).collect()).unwrap_or_default();
            (contents, rules, scripts)
        });
        let user_script = |source: &str, world: &WKContentWorld| unsafe {
            WKUserScript::initWithSource_injectionTime_forMainFrameOnly_inContentWorld(
                WKUserScript::alloc(mtm),
                &NSString::from_str(source),
                WKUserScriptInjectionTime::AtDocumentStart,
                false,
                world,
            )
        };
        let page_world = unsafe { WKContentWorld::pageWorld(mtm) };
        let ask = user_script(ASK_SCRIPT, &page_world);
        let frame = user_script(FRAME_SCRIPT, &chain_world(mtm));
        let scripts: Vec<_> = scripts.iter().map(|source| user_script(source, &page_world)).collect();
        for content in contents {
            unsafe {
                content.removeAllContentRuleLists();
                for list in &rules {
                    content.addContentRuleList(list);
                }
                content.removeAllUserScripts();
                content.addUserScript(&ask);
                content.addUserScript(&frame);
                for script in &scripts {
                    content.addUserScript(script);
                }
            }
        }
    }

    fn rule_store(mtm: MainThreadMarker) -> Result<Retained<WKContentRuleListStore>, BrowserViewError> {
        unsafe { WKContentRuleListStore::defaultStore(mtm) }.ok_or_else(|| BrowserViewError::Other("the engine has no rule store".to_string()))
    }

    // Deletes the list's compiled versions other than `keep`.
    fn remove_compiled(store: &WKContentRuleListStore, prefix: String, keep: Option<String>) {
        let remover = store.retain();
        let block = RcBlock::new(move |identifiers: *mut NSArray<NSString>| {
            let Some(identifiers) = (unsafe { identifiers.as_ref() }) else { return };
            for identifier in identifiers.iter().map(|identifier| identifier.to_string()) {
                if identifier.starts_with(&prefix) && keep.as_deref() != Some(&identifier) {
                    // WebKit calls this handler without checking it for nil.
                    let removed = RcBlock::new(|_error: *mut NSError| {});
                    unsafe { remover.removeContentRuleListForIdentifier_completionHandler(Some(&NSString::from_str(&identifier)), Some(&removed)) };
                }
            }
        });
        unsafe { store.getAvailableContentRuleListIdentifiers(Some(&block)) };
    }

    // WebKit puts the compiler's own reason in NSHelpAnchor.
    fn compile_error(error: &NSError) -> String {
        error
            .userInfo()
            .objectForKey(ns_string!("NSHelpAnchor"))
            .and_then(|reason| reason.downcast::<NSString>().ok())
            .map_or_else(|| error.localizedDescription().to_string(), |reason| reason.to_string())
    }

    type RulesDone = Box<dyn FnOnce(Result<(), BrowserViewError>)>;

    // One setRules() in flight: look its compiled copy up, compile if
    // there's none, then put it on the session's views.
    struct SettingRules {
        session: String,
        id: String,
        prefix: String,
        identifier: String,
        rules_json: String,
        store: Retained<WKContentRuleListStore>,
        done: std::cell::Cell<Option<RulesDone>>,
    }

    impl SettingRules {
        fn finish(&self, result: Result<(), BrowserViewError>) {
            if let Some(done) = self.done.take() {
                done(result);
            }
        }

        fn look_up(self: &std::rc::Rc<Self>) {
            let this = self.clone();
            let found = RcBlock::new(move |list: *mut WKContentRuleList, _error: *mut NSError| match unsafe { Retained::retain(list) } {
                Some(list) => this.install(list),
                None => this.compile(),
            });
            unsafe { self.store.lookUpContentRuleListForIdentifier_completionHandler(Some(&NSString::from_str(&self.identifier)), Some(&found)) };
        }

        fn compile(self: &std::rc::Rc<Self>) {
            let this = self.clone();
            let compiled = RcBlock::new(move |list: *mut WKContentRuleList, error: *mut NSError| match unsafe { Retained::retain(list) } {
                Some(list) => this.install(list),
                None => {
                    let why = unsafe { error.as_ref() }.map(compile_error).unwrap_or_default();
                    this.finish(Err(BrowserViewError::InvalidArgument(format!("the engine refused the rules: {why}"))));
                }
            });
            unsafe {
                self.store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(
                    Some(&NSString::from_str(&self.identifier)),
                    Some(&NSString::from_str(&self.rules_json)),
                    Some(&compiled),
                )
            };
        }

        fn install(&self, list: Retained<WKContentRuleList>) {
            with(|r| {
                let rules = &mut r.sessions.entry(self.session.clone()).or_default().rules;
                match rules.iter_mut().find(|(id, _)| *id == self.id) {
                    Some(entry) => entry.1 = list,
                    None => rules.push((self.id.clone(), list)),
                }
            });
            put_session_content(&self.session);
            remove_compiled(&self.store, self.prefix.clone(), Some(self.identifier.clone()));
            self.finish(Ok(()));
        }
    }

    /// Puts `rules_json` on the session's views as list `id`, compiling it
    /// unless an identical list is already compiled. `done` runs on the
    /// main thread. Main thread.
    pub fn set_rules(
        session: &str,
        session_key: [u8; 16],
        id: &str,
        rules_json: String,
        done: impl FnOnce(Result<(), BrowserViewError>) + 'static,
    ) {
        let store = match MainThreadMarker::new().ok_or_else(|| BrowserViewError::Other("rules are set on the main thread".to_string())).and_then(rule_store) {
            Ok(store) => store,
            Err(e) => return done(Err(e)),
        };
        let (prefix, identifier) = rule_list_identifier(session_key, id, &rules_json);
        std::rc::Rc::new(SettingRules {
            session: session.to_string(),
            id: id.to_string(),
            prefix,
            identifier,
            rules_json,
            store,
            done: std::cell::Cell::new(Some(Box::new(done))),
        })
        .look_up();
    }

    /// Takes list `id` off the session's views and deletes its compiled copy.
    pub fn remove_rules(session: &str, session_key: [u8; 16], id: &str) {
        with(|r| {
            if let Some(content) = r.sessions.get_mut(session) {
                content.rules.retain(|(existing, _)| existing != id);
            }
        });
        put_session_content(session);
        if let Some(store) = MainThreadMarker::new().and_then(|mtm| rule_store(mtm).ok()) {
            remove_compiled(&store, rule_list_identifier(session_key, id, "").0, None);
        }
    }

    /// Adds or replaces script `id`, keeping its place. Main thread.
    pub fn set_script(session: &str, id: &str, source: &str) {
        with(|r| {
            let scripts = &mut r.sessions.entry(session.to_string()).or_default().scripts;
            match scripts.iter_mut().find(|(existing, _)| existing == id) {
                Some(entry) => entry.1 = source.to_string(),
                None => scripts.push((id.to_string(), source.to_string())),
            }
        });
        put_session_content(session);
    }

    pub fn remove_script(session: &str, id: &str) {
        with(|r| {
            if let Some(content) = r.sessions.get_mut(session) {
                content.scripts.retain(|(existing, _)| existing != id);
            }
        });
        put_session_content(session);
    }

    fn frame_error(error: &NSError) -> String {
        let info = error.userInfo();
        info.objectForKey(ns_string!("WKJavaScriptExceptionMessage"))
            .and_then(|message| message.downcast::<NSString>().ok())
            .map_or_else(|| error.localizedDescription().to_string(), |message| message.to_string())
    }

    /// Runs `body` in the view's page and every frame that reported in, in
    /// Chain's own world, and hands `done` each frame's result, page first.
    /// A frame that's gone is reported once and forgotten. Main thread.
    pub fn run_in_frames(id: &str, body: &str, done: impl FnOnce(Result<Vec<FrameResult>, BrowserViewError>) + 'static) {
        let Some(mtm) = MainThreadMarker::new() else {
            return done(Err(BrowserViewError::Other("frames are run on the main thread".to_string())));
        };
        let Some((webview, frames)) = with(|r| r.views.get(id).map(|view| (view.webview.clone(), view.frames.clone()))) else {
            return done(Err(not_found(id)));
        };
        let targets: Vec<(Option<Retained<WKFrameInfo>>, Frame)> = std::iter::once((None, Frame { url: webview_url(&webview), main_frame: true }))
            .chain(frames.into_iter().map(|frame| {
                let described = frame_of(&frame);
                (Some(frame), described)
            }))
            .collect();
        let results = std::rc::Rc::new(std::cell::RefCell::new(vec![None; targets.len()]));
        let done = std::rc::Rc::new(std::cell::Cell::new(Some(Box::new(done) as Box<dyn FnOnce(Result<Vec<FrameResult>, BrowserViewError>)>)));
        let source = NSString::from_str(&frame_body(body));
        let world = chain_world(mtm);
        for (index, (frame, described)) in targets.into_iter().enumerate() {
            let (results, done, view_id, kept) = (results.clone(), done.clone(), id.to_string(), frame.clone());
            let finished = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
                let outcome = match (unsafe { value.as_ref() }.and_then(|v| v.downcast_ref::<NSString>()), unsafe { error.as_ref() }) {
                    (_, Some(error)) => {
                        let gone = error.domain().to_string() == "WKErrorDomain" && error.code() == WKErrorCode::JavaScriptInvalidFrameTarget.0;
                        if let (true, Some(kept)) = (gone, &kept) {
                            with(|r| {
                                if let Some(view) = r.views.get_mut(&view_id) {
                                    view.frames.retain(|known| !known.isEqual(Some(kept)));
                                }
                            });
                        }
                        FrameResult { frame: described.clone(), value: None, error: Some(frame_error(error)) }
                    }
                    (Some(json), None) => match serde_json::from_str(&json.to_string()) {
                        Ok(value) => FrameResult { frame: described.clone(), value: Some(value), error: None },
                        Err(e) => FrameResult { frame: described.clone(), value: None, error: Some(e.to_string()) },
                    },
                    (None, None) => FrameResult { frame: described.clone(), value: Some(serde_json::Value::Null), error: None },
                };
                let mut results = results.borrow_mut();
                results[index] = Some(outcome);
                if results.iter().all(Option::is_some) {
                    if let Some(done) = done.take() {
                        done(Ok(results.drain(..).flatten().collect()));
                    }
                }
            });
            unsafe {
                webview.callAsyncJavaScript_arguments_inFrame_inContentWorld_completionHandler(&source, None, frame.as_deref(), &world, Some(&finished))
            };
        }
    }

    fn not_found(id: &str) -> BrowserViewError {
        BrowserViewError::NotFound(format!("no view {id:?}"))
    }

    fn webview(id: &str) -> Result<Retained<WKWebView>, BrowserViewError> {
        with(|r| r.views.get(id).map(|view| view.webview.clone())).ok_or_else(|| not_found(id))
    }

    fn app_navigation(id: &str) {
        with(|r| {
            if let Some(view) = r.views.get_mut(id) {
                view.app_navigation = true;
            }
        });
    }

    /// Main thread, like everything below.
    pub fn navigate(id: &str, url: &str) -> Result<(), BrowserViewError> {
        let webview = webview(id)?;
        let url = NSURL::URLWithString(&NSString::from_str(url))
            .ok_or_else(|| BrowserViewError::InvalidArgument(format!("invalid URL {url:?}")))?;
        app_navigation(id);
        unsafe { webview.loadRequest(&NSURLRequest::requestWithURL(&url)) };
        Ok(())
    }

    /// Back when `back`, else forward; nothing when there's nowhere to go.
    pub fn go(id: &str, back: bool) -> Result<(), BrowserViewError> {
        let webview = webview(id)?;
        unsafe {
            if (back && webview.canGoBack()) || (!back && webview.canGoForward()) {
                app_navigation(id);
                if back {
                    webview.goBack();
                } else {
                    webview.goForward();
                }
            }
        }
        Ok(())
    }

    /// After a crash, loads the last address shown again.
    pub fn reload(id: &str) -> Result<(), BrowserViewError> {
        let webview = webview(id)?;
        if unsafe { webview.URL() }.is_some() {
            app_navigation(id);
            unsafe { webview.reload() };
            return Ok(());
        }
        match with(|r| r.views.get(id).and_then(|view| view.last_url.clone())) {
            Some(url) => navigate(id, &url),
            None => Ok(()),
        }
    }

    pub fn stop(id: &str) -> Result<(), BrowserViewError> {
        unsafe { webview(id)?.stopLoading() };
        Ok(())
    }

    pub fn state(id: &str) -> Result<State, BrowserViewError> {
        Ok(read_state(id, &*webview(id)?))
    }

    /// Lets a held navigation go ahead or refuses it; nothing once it's
    /// answered, expired, or its view closed.
    pub fn answer(request_id: &str, allow: bool) {
        let decide = with(|r| {
            let pending = r.pending.remove(request_id)?;
            if let (true, Some(view)) = (allow, r.views.get_mut(&pending.view)) {
                view.provisional = true;
            }
            Some(pending.decide)
        });
        if let Some(decide) = decide {
            decide.call((if allow { WKNavigationActionPolicy::Allow } else { WKNavigationActionPolicy::Cancel },));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> reqwest::Url {
        reqwest::Url::parse(s).unwrap()
    }

    fn rule(url_filter: &str, action: RuleAction) -> Rule {
        Rule {
            trigger: RuleTrigger {
                url_filter: url_filter.to_string(),
                url_filter_is_case_sensitive: None,
                resource_type: None,
                load_type: None,
                if_domain: None,
                unless_domain: None,
                if_top_url: None,
                unless_top_url: None,
            },
            action,
        }
    }

    #[test]
    fn rules_read_camel_case_and_write_webkit_json() {
        let rules: Vec<Rule> = serde_json::from_str(
            r#"[{"trigger":{"urlFilter":"ads\\.js","resourceType":["script","style-sheet"],"loadType":["third-party"],"unlessDomain":["*youtube.com"]},"action":{"type":"block"}},
                {"trigger":{"urlFilter":".*","ifDomain":["example.com"]},"action":{"type":"css-display-none","selector":".banner"}},
                {"trigger":{"urlFilter":"good\\.js"},"action":{"type":"ignore-previous-rules"}}]"#,
        )
        .unwrap();
        let json: serde_json::Value = serde_json::from_str(&content_rules_json(&rules).unwrap()).unwrap();
        assert_eq!(
            json[0],
            serde_json::json!({"trigger":{"url-filter":"ads\\.js","resource-type":["script","style-sheet"],"load-type":["third-party"],"unless-domain":["*youtube.com"]},"action":{"type":"block"}})
        );
        assert_eq!(json[1]["action"], serde_json::json!({"type":"css-display-none","selector":".banner"}));
        assert_eq!(json[2]["action"], serde_json::json!({"type":"ignore-previous-rules"}));
    }

    #[test]
    fn rules_outside_the_subset_are_refused() {
        assert!(serde_json::from_str::<Rule>(r#"{"trigger":{"urlFilter":"x"},"action":{"type":"redirect"}}"#).is_err());
        assert!(serde_json::from_str::<Rule>(r#"{"trigger":{"urlFilter":"x","resourceType":["raw"]},"action":{"type":"block"}}"#).is_err());
        assert!(content_rules_json(&[]).is_err());
        assert!(matches!(content_rules_json(&[rule("a|b", RuleAction::Block)]), Err(BrowserViewError::InvalidArgument(m)) if m.contains('|')));
        assert!(content_rules_json(&[rule("a{2}", RuleAction::Block)]).is_err());
        assert!(content_rules_json(&[rule(r"[|{]x\|y", RuleAction::Block)]).is_ok());
        let parsed = parse_rules(vec![
            serde_json::json!({"trigger":{"urlFilter":"x"},"action":{"type":"block"}}),
            serde_json::json!({"trigger":{"urlFilter":"x"},"action":{"type":"redirect"}}),
        ]);
        assert!(matches!(parsed, Err(BrowserViewError::InvalidArgument(m)) if m.starts_with("rule 1:")));
        assert!(matches!(content_rules_json(&[rule("", RuleAction::Block)]), Err(BrowserViewError::InvalidArgument(m)) if m.starts_with("rule 0")));
        let css = rule(".*", RuleAction::CssDisplayNone { selector: " ".to_string() });
        assert!(content_rules_json(&[css]).is_err());
        let mut both = rule("x", RuleAction::Block);
        both.trigger.if_domain = Some(vec!["a.com".to_string()]);
        both.trigger.unless_domain = Some(vec!["b.com".to_string()]);
        assert!(matches!(content_rules_json(&[rule("x", RuleAction::Block), both]), Err(BrowserViewError::InvalidArgument(m)) if m.starts_with("rule 1")));
    }

    #[test]
    fn compiled_lists_are_named_by_session_list_and_content() {
        let a = [1u8; 16];
        let (prefix, first) = rule_list_identifier(a, "ads", "[1]");
        let (same_prefix, changed) = rule_list_identifier(a, "ads", "[2]");
        assert_eq!(prefix, same_prefix);
        assert!(first.starts_with(&prefix) && changed.starts_with(&prefix));
        assert_ne!(first, changed);
        assert_eq!(rule_list_identifier(a, "ads", "[1]").1, first);
        assert_ne!(rule_list_identifier(a, "cosmetic", "[1]").0, prefix);
        assert_ne!(rule_list_identifier([2u8; 16], "ads", "[1]").0, prefix);
    }

    #[test]
    fn views_show_the_web_and_previews_but_never_the_app() {
        let dev = url("http://localhost:1420/");
        assert!(may_show(&url("https://example.com/a"), Some(&dev)));
        assert!(may_show(&url("http://localhost:5173/"), Some(&dev)));
        assert!(may_show(&url("http://127.0.0.1:3000/"), Some(&dev)));
        assert!(may_show(&url("about:blank"), Some(&dev)));
        assert!(may_show(&url("data:text/html,hi"), Some(&dev)));
        assert!(!may_show(&url("http://localhost:1420/settings"), Some(&dev)));
        assert!(!may_show(&url("blob:http://localhost:1420/1234"), Some(&dev)));
        assert!(!may_show(&url("http://tauri.localhost/"), None));
        assert!(!may_show(&url("tauri://localhost/"), None));
        assert!(!may_show(&url("file:///etc/hosts"), None));
        assert!(!may_show(&url("javascript:alert(1)"), None));
        assert!(matches!(view_url("not a url", None), Err(BrowserViewError::InvalidArgument(_))));
        assert!(matches!(view_url("http://localhost:1420/", Some(&dev)), Err(BrowserViewError::InvalidArgument(_))));
    }

    #[test]
    fn bounds_are_checked_and_follow_page_zoom() {
        let bounds = Bounds { x: 10.0, y: 40.0, width: 300.0, height: 200.0 };
        assert_eq!(bounds.checked().unwrap(), bounds);
        assert!(Bounds { width: 0.0, height: 0.0, ..bounds }.checked().is_ok());
        assert!(Bounds { width: -1.0, ..bounds }.checked().is_err());
        assert!(Bounds { x: f64::NAN, ..bounds }.checked().is_err());
        assert_eq!(bounds.in_window(0.0, 28.0, 1.5), Bounds { x: 15.0, y: 88.0, width: 450.0, height: 300.0 });
    }

    #[test]
    fn interrupted_loads_are_not_failures() {
        assert_eq!(failure_reason("NSURLErrorDomain", -999), None);
        assert_eq!(failure_reason("WebKitErrorDomain", 102), None);
        assert_eq!(failure_reason("NSURLErrorDomain", -1009), Some(FailureReason::Offline));
        assert_eq!(failure_reason("NSURLErrorDomain", -1003), Some(FailureReason::HostNotFound));
        assert_eq!(failure_reason("NSURLErrorDomain", -1004), Some(FailureReason::Unreachable));
        assert_eq!(failure_reason("NSURLErrorDomain", -1202), Some(FailureReason::InsecureConnection));
        assert_eq!(failure_reason("WebKitErrorDomain", 103), Some(FailureReason::Blocked));
        assert_eq!(failure_reason("WebKitErrorDomain", 104), Some(FailureReason::Blocked));
        assert_eq!(failure_reason("NSURLErrorDomain", -1100), Some(FailureReason::Other));
        assert_eq!(serde_json::to_value(FailureReason::HostNotFound).unwrap(), "host-not-found");
    }

    #[test]
    fn initiator_details_read_like_the_web() {
        assert_eq!(navigation_kind(0), NavigationKind::Link);
        assert_eq!(navigation_kind(4), NavigationKind::Form);
        assert_eq!(navigation_kind(2), NavigationKind::History);
        assert_eq!(navigation_kind(-1), NavigationKind::Other);
        assert_eq!(mouse_button(1), Some(MouseButton::Primary));
        assert_eq!(mouse_button(4), Some(MouseButton::Middle));
        assert_eq!(mouse_button(0), None);
        assert_eq!(modifiers(1 << 20 | 1 << 17), Modifiers { shift: true, meta: true, ..Modifiers::default() });
    }

    #[test]
    fn fragment_jumps_stay_in_the_document() {
        assert!(same_document("https://a.com/p", "https://a.com/p#top"));
        assert!(same_document("https://a.com/p#x", "https://a.com/p#y"));
        assert!(!same_document("https://a.com/p#x", "https://a.com/p"));
        assert!(!same_document("https://a.com/p", "https://a.com/q#top"));
    }

    #[test]
    fn requests_serialize_flat_and_camel_cased() {
        let request = NavigationRequest {
            id: "1".to_string(),
            request_id: "7".to_string(),
            url: "https://a.com/".to_string(),
            kind: NavigationKind::Link,
            initiator: Initiator {
                source_url: "https://b.com/".to_string(),
                from_main_frame: false,
                modifiers: Modifiers::default(),
                button: Some(MouseButton::Middle),
                input_age_ms: None,
            },
        };
        let json = serde_json::to_value(request).unwrap();
        assert_eq!(json["requestId"], "7");
        assert_eq!(json["fromMainFrame"], false);
        assert_eq!(json["button"], "middle");
        assert!(json["inputAgeMs"].is_null());
        let features = serde_json::to_value(WindowFeatures { width: Some(400.0), ..WindowFeatures::default() }).unwrap();
        assert_eq!(features, serde_json::json!({ "width": 400.0 }));
    }
}
