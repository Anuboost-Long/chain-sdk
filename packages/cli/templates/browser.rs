// Bridges the browser capability contract (capabilities/browser in
// chain-sdk): a separate signed-in browser window. The window, its two
// webviews (toolbar on top, the page below) and their events are Tauri's,
// so they live here; naming, navigation rules, reading and the session
// fetch are chain_core::browser. See agent-docs/capabilities/browser/.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

use chain_core::browser::{self as core, BrowserError, Button};
use tauri::webview::{NewWindowFeatures, NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::{Emitter, LogicalPosition, LogicalSize, Manager, Url, WebviewUrl};

const TOOLBAR_HEIGHT: f64 = 44.0;
const READ_TIMEOUT: Duration = Duration::from_secs(10);
const COOKIE_PAGE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct BrowserState {
    sessions: Mutex<HashMap<String, Session>>,
    // The same for every webview in the app; sent with session fetches.
    user_agent: Mutex<Option<String>>,
}

struct Session {
    label: String,
    fixed_title: bool,
    title: String,
    buttons: Vec<Button>,
    closed_by_app: bool,
    // The last page onNavigate reported, so a load and its title change
    // don't both report the same page.
    reported: Option<(String, String)>,
}

fn page_label(label: &str) -> String {
    format!("{label}-page")
}

fn toolbar_label(label: &str) -> String {
    format!("{label}-toolbar")
}

static NEXT_POPUP: AtomicU32 = AtomicU32::new(0);

// The SDK maps these prefixes onto ChainErrorCode — see packages/sdk/src/browser.ts.
fn to_browser_command_error(e: BrowserError) -> String {
    match e {
        BrowserError::InvalidArgument(m) => format!("INVALID_ARGUMENT: {m}"),
        BrowserError::NotFound(m) => format!("NOT_FOUND: {m}"),
        BrowserError::Unavailable(m) => format!("UNAVAILABLE: {m}"),
        BrowserError::Unsupported(m) => format!("UNSUPPORTED: {m}"),
        BrowserError::Other(m) => m,
    }
}

fn session_name(session: Option<String>) -> Result<String, String> {
    core::session_name(session.as_deref()).map_err(to_browser_command_error)
}

fn not_open(session: &str) -> String {
    format!("NOT_FOUND: no browser window is open for session {session:?}")
}

fn supported() -> bool {
    #[cfg(target_os = "macos")]
    return core::store_supported();
    #[cfg(windows)]
    return true;
    #[cfg(not(any(target_os = "macos", windows)))]
    return false;
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    available: bool,
    toolbar: bool,
    buttons: bool,
    popups: bool,
    persistent_sessions: bool,
    named_sessions: bool,
    clear_session: bool,
    read_content: bool,
    read_frames: bool,
    fetch: bool,
    open_beside: bool,
}

#[tauri::command]
pub fn browser_availability() -> Availability {
    let all = supported();
    Availability {
        available: all,
        toolbar: all,
        buttons: all,
        popups: all,
        persistent_sessions: all,
        named_sessions: all,
        clear_session: all,
        read_content: all,
        read_frames: all,
        fetch: all,
        open_beside: all,
    }
}

#[derive(Clone, serde::Serialize)]
pub struct Page {
    session: String,
    url: String,
    title: String,
}

#[derive(Clone, serde::Serialize)]
struct ButtonPayload {
    session: String,
    id: String,
    url: String,
    title: String,
}

#[derive(Clone, serde::Serialize)]
struct ClosedPayload {
    session: String,
    reason: &'static str,
}

fn update_toolbar(app: &tauri::AppHandle, label: &str, state: serde_json::Value) {
    if let Some(toolbar) = app.get_webview(&toolbar_label(label)) {
        let _ = toolbar.eval(format!("window.chainToolbar?.update({state})"));
    }
}

// macOS reads WKWebView's own history state; elsewhere both buttons stay
// enabled (see research/WINDOWS.md).
fn refresh_history(app: &tauri::AppHandle, label: &str) {
    #[cfg(target_os = "macos")]
    if let (Some(page), Some(toolbar)) = (app.get_webview(&page_label(label)), app.get_webview(&toolbar_label(label))) {
        let _ = page.with_webview(move |webview| {
            let (back, forward) = core::can_go(webview.inner());
            let state = serde_json::json!({ "canGoBack": back, "canGoForward": forward });
            let _ = toolbar.eval(format!("window.chainToolbar?.update({state})"));
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, label);
}

fn current_page(app: &tauri::AppHandle, session: &str) -> Option<Page> {
    let state = app.state::<BrowserState>();
    let sessions = state.sessions.lock().expect("browser mutex poisoned");
    let open = sessions.get(session)?;
    let url = app.get_webview(&page_label(&open.label))?.url().ok()?;
    Some(Page { session: session.to_string(), url: url.to_string(), title: open.title.clone() })
}

fn page_changed(app: &tauri::AppHandle, session: &str) {
    let Some(page) = current_page(app, session) else { return };
    let label = core::session_label(session);
    update_toolbar(app, &label, serde_json::json!({ "url": page.url }));
    refresh_history(app, &label);
    let unreported = {
        let state = app.state::<BrowserState>();
        let mut sessions = state.sessions.lock().expect("browser mutex poisoned");
        let Some(open) = sessions.get_mut(session) else { return };
        let shown = Some((page.url.clone(), page.title.clone()));
        let unreported = open.reported != shown;
        open.reported = shown;
        unreported
    };
    if unreported {
        let _ = app.emit("chain://browser-navigated", page);
    }
}

fn remember_user_agent(app: &tauri::AppHandle, webview: &tauri::Webview) {
    if app.state::<BrowserState>().user_agent.lock().expect("browser mutex poisoned").is_some() {
        return;
    }
    let app = app.clone();
    let _ = webview.eval_with_callback("navigator.userAgent", move |result| {
        if let Ok(agent) = serde_json::from_str::<String>(&result) {
            *app.state::<BrowserState>().user_agent.lock().expect("browser mutex poisoned") = Some(agent);
        }
    });
}

fn navigation_guard(app: &tauri::AppHandle) -> impl Fn(&Url) -> bool + Send + 'static {
    let app_origin = app.config().build.dev_url.clone();
    move |url| core::may_navigate(url, app_origin.as_ref())
}

type PopupHandler = Box<dyn Fn(Url, NewWindowFeatures) -> NewWindowResponse<tauri::Wry> + Send>;

// Sign-in popups: WebKit/WebView2 load the URL into the window we return,
// built from `features` so it shares the opener's session and keeps
// `window.opener`. Popups of popups go through here too.
fn popup_handler(app: tauri::AppHandle, label: String) -> PopupHandler {
    Box::new(move |url, features| {
        if !navigation_guard(&app)(&url) {
            return NewWindowResponse::Deny;
        }
        let popup = format!("{label}-popup-{}", NEXT_POPUP.fetch_add(1, Ordering::Relaxed));
        let blank = WebviewUrl::External("about:blank".parse().expect("valid URL"));
        let built = tauri::WebviewWindowBuilder::new(&app, popup, blank)
            .window_features(features)
            .title(url.host_str().unwrap_or_default())
            .on_navigation(navigation_guard(&app))
            .on_new_window(popup_handler(app.clone(), label.clone()))
            .on_document_title_changed(|window, title| {
                let _ = window.set_title(&title);
            })
            .build();
        match built {
            Ok(window) => NewWindowResponse::Create { window },
            Err(e) => {
                eprintln!("[chain] couldn't open a sign-in popup: {e}");
                NewWindowResponse::Deny
            }
        }
    })
}

fn title_bar_height(window: &tauri::Window) -> f64 {
    #[cfg(target_os = "macos")]
    return window.ns_window().map_or(0.0, core::title_bar_height);
    #[cfg(not(target_os = "macos"))]
    return {
        let _ = window;
        0.0
    };
}

// Main thread (macOS reads the title bar from the NSWindow).
fn layout(window: &tauri::Window, label: &str, toolbar: bool) {
    let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) else { return };
    let size = size.to_logical::<f64>(scale);
    let title_bar = title_bar_height(window);
    let page_top = title_bar + if toolbar { TOOLBAR_HEIGHT } else { 0.0 };
    if let Some(bar) = window.get_webview(&toolbar_label(label)) {
        let _ = bar.set_position(LogicalPosition::new(0.0, title_bar));
        let _ = bar.set_size(LogicalSize::new(size.width, TOOLBAR_HEIGHT));
    }
    if let Some(page) = window.get_webview(&page_label(label)) {
        let _ = page.set_position(LogicalPosition::new(0.0, page_top));
        let _ = page.set_size(LogicalSize::new(size.width, (size.height - page_top).max(0.0)));
    }
}

fn beside_position(app_window: &tauri::Window, width: f64, height: f64) -> Option<(f64, f64)> {
    let scale = app_window.scale_factor().ok()?;
    let position = app_window.outer_position().ok()?.to_logical::<f64>(scale);
    let size = app_window.outer_size().ok()?.to_logical::<f64>(scale);
    let monitor = app_window.current_monitor().ok()??;
    let area = monitor.work_area();
    let origin = area.position.to_logical::<f64>(scale);
    let extent = area.size.to_logical::<f64>(scale);
    core::beside(
        core::Rect { x: position.x, y: position.y, width: size.width, height: size.height },
        width,
        height,
        core::Rect { x: origin.x, y: origin.y, width: extent.width, height: extent.height },
    )
}

#[cfg(target_os = "macos")]
fn with_session_store<R: tauri::Runtime>(builder: WebviewBuilder<R>, _app: &tauri::AppHandle, session: &str) -> Result<WebviewBuilder<R>, String> {
    Ok(builder.data_store_identifier(core::session_key(session)))
}

#[cfg(not(target_os = "macos"))]
fn with_session_store<R: tauri::Runtime>(builder: WebviewBuilder<R>, app: &tauri::AppHandle, session: &str) -> Result<WebviewBuilder<R>, String> {
    let dir = app.path().app_local_data_dir().map_err(|e| format!("UNAVAILABLE: {e}"))?;
    Ok(builder.data_directory(core::session_data_dir(&dir, session)))
}

fn toolbar_url() -> Url {
    #[cfg(windows)]
    let url = "http://chain-browser.localhost/toolbar";
    #[cfg(not(windows))]
    let url = "chain-browser://localhost/toolbar";
    url.parse().expect("valid URL")
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenOptions {
    session: Option<String>,
    url: Option<String>,
    title: Option<String>,
    width: Option<f64>,
    height: Option<f64>,
    min_width: Option<f64>,
    min_height: Option<f64>,
    beside: Option<bool>,
    toolbar: Option<bool>,
    buttons: Option<Vec<Button>>,
}

// Async: WebView2 deadlocks creating webviews from a synchronous command.
#[tauri::command]
pub async fn browser_open(app: tauri::AppHandle, window: tauri::Window, options: OpenOptions) -> Result<(), String> {
    if !supported() {
        return Err("UNSUPPORTED: the browser window needs macOS 14 or later, or Windows".to_string());
    }
    let session = session_name(options.session)?;
    let url = options.url.as_deref().map(core::web_url).transpose().map_err(to_browser_command_error)?;
    if let Some(buttons) = &options.buttons {
        core::validate_buttons(buttons).map_err(to_browser_command_error)?;
    }
    let dimension = |name, value, default| core::dimension(name, value, default).map_err(to_browser_command_error);
    let width = dimension("width", options.width, 1100.0)?;
    let height = dimension("height", options.height, 800.0)?;
    let min_width = dimension("minWidth", options.min_width, 480.0)?;
    let min_height = dimension("minHeight", options.min_height, 360.0)?;

    let label = core::session_label(&session);
    if let Some(open) = app.get_window(&label) {
        if let Some(buttons) = options.buttons {
            set_buttons(&app, &session, buttons)?;
        }
        if let (Some(url), Some(page)) = (url, app.get_webview(&page_label(&label))) {
            page.navigate(url).map_err(|e| e.to_string())?;
        }
        let _ = open.unminimize();
        let _ = open.show();
        let _ = open.set_focus();
        return Ok(());
    }
    let url = url.ok_or_else(|| "INVALID_ARGUMENT: url is required to open the window".to_string())?;

    let toolbar = options.toolbar.unwrap_or(true);
    let window_title = options.title.clone().unwrap_or_else(|| url.host_str().unwrap_or_default().to_string());
    app.state::<BrowserState>().sessions.lock().expect("browser mutex poisoned").insert(
        session.clone(),
        Session {
            label: label.clone(),
            fixed_title: options.title.is_some(),
            title: String::new(),
            buttons: options.buttons.unwrap_or_default(),
            closed_by_app: false,
            reported: None,
        },
    );
    let opened = build_window(&app, &window, &session, &label, url, window_title, (width, height), (min_width, min_height), options.beside.unwrap_or(true), toolbar);
    if opened.is_err() {
        app.state::<BrowserState>().sessions.lock().expect("browser mutex poisoned").remove(&session);
        if let Some(partial) = app.get_window(&label) {
            let _ = partial.destroy();
        }
    }
    opened
}

#[allow(clippy::too_many_arguments)]
fn build_window(
    app: &tauri::AppHandle,
    app_window: &tauri::Window,
    session: &str,
    label: &str,
    url: Url,
    title: String,
    (width, height): (f64, f64),
    (min_width, min_height): (f64, f64),
    beside: bool,
    toolbar: bool,
) -> Result<(), String> {
    let builder = tauri::window::WindowBuilder::new(app, label)
        .title(title)
        .inner_size(width, height)
        .min_inner_size(min_width, min_height);
    let builder = match beside.then(|| beside_position(app_window, width, height)).flatten() {
        Some((x, y)) => builder.position(x, y),
        None => builder.center(),
    };
    let window = builder.build().map_err(|e| e.to_string())?;

    let top = if toolbar { TOOLBAR_HEIGHT } else { 0.0 };
    if toolbar {
        let expected = toolbar_url();
        let bar = WebviewBuilder::new(toolbar_label(label), WebviewUrl::CustomProtocol(expected.clone()))
            .on_navigation(move |url| url == &expected);
        window
            .add_child(bar, LogicalPosition::new(0.0, 0.0), LogicalSize::new(width, top))
            .map_err(|e| e.to_string())?;
    }

    let on_load = (app.clone(), session.to_string());
    let on_title = (app.clone(), session.to_string());
    let page = WebviewBuilder::new(page_label(label), WebviewUrl::External(url))
        .on_navigation(navigation_guard(app))
        .on_new_window(popup_handler(app.clone(), label.to_string()))
        .on_page_load(move |webview, payload| {
            let (app, session) = &on_load;
            match payload.event() {
                PageLoadEvent::Started => {
                    let label = core::session_label(session);
                    update_toolbar(app, &label, serde_json::json!({ "url": payload.url().as_str() }));
                }
                PageLoadEvent::Finished => {
                    remember_user_agent(app, &webview);
                    page_changed(app, session);
                }
            }
        })
        .on_document_title_changed(move |webview, title| {
            let (app, session) = &on_title;
            let fixed = {
                let state = app.state::<BrowserState>();
                let mut sessions = state.sessions.lock().expect("browser mutex poisoned");
                let Some(open) = sessions.get_mut(session) else { return };
                open.title = title.clone();
                open.fixed_title
            };
            // A page's title empties while the next one loads.
            if title.is_empty() {
                return;
            }
            if !fixed {
                let _ = webview.window().set_title(&title);
            }
            page_changed(app, session);
        });
    let page = with_session_store(page, app, session)?;
    let page = window
        .add_child(page, LogicalPosition::new(0.0, top), LogicalSize::new(width, height - top))
        .map_err(|e| e.to_string())?;
    // WebView2 already closes a popup that calls window.close().
    #[cfg(target_os = "macos")]
    let _ = page.with_webview(|webview| core::close_on_window_close(webview.inner()));
    #[cfg(not(target_os = "macos"))]
    let _ = page;

    let events = (app.clone(), session.to_string(), label.to_string());
    window.on_window_event(move |event| {
        let (app, session, label) = &events;
        match event {
            tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = app.get_window(label) {
                    layout(&window, label, toolbar);
                }
            }
            tauri::WindowEvent::Destroyed => window_closed(app, session, label),
            _ => {}
        }
    });
    let (laid_out, label) = (window.clone(), label.to_string());
    let _ = app.run_on_main_thread(move || layout(&laid_out, &label, toolbar));
    Ok(())
}

fn window_closed(app: &tauri::AppHandle, session: &str, label: &str) {
    let closed = app.state::<BrowserState>().sessions.lock().expect("browser mutex poisoned").remove(session);
    let popups = format!("{label}-popup-");
    for (popup, window) in app.webview_windows() {
        if popup.starts_with(&popups) {
            let _ = window.close();
        }
    }
    if let Some(closed) = closed {
        let reason = if closed.closed_by_app { "app" } else { "user" };
        let _ = app.emit("chain://browser-closed", ClosedPayload { session: session.to_string(), reason });
    }
}

#[tauri::command]
pub async fn browser_close(app: tauri::AppHandle, session: Option<String>) -> Result<(), String> {
    let session = session_name(session)?;
    let label = {
        let state = app.state::<BrowserState>();
        let mut sessions = state.sessions.lock().expect("browser mutex poisoned");
        let Some(open) = sessions.get_mut(&session) else { return Ok(()) };
        open.closed_by_app = true;
        open.label.clone()
    };
    if let Some(window) = app.get_window(&label) {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn set_buttons(app: &tauri::AppHandle, session: &str, buttons: Vec<Button>) -> Result<(), String> {
    core::validate_buttons(&buttons).map_err(to_browser_command_error)?;
    let label = {
        let state = app.state::<BrowserState>();
        let mut sessions = state.sessions.lock().expect("browser mutex poisoned");
        let open = sessions.get_mut(session).ok_or_else(|| not_open(session))?;
        open.buttons = buttons.clone();
        open.label.clone()
    };
    update_toolbar(app, &label, serde_json::json!({ "buttons": buttons }));
    Ok(())
}

#[tauri::command]
pub async fn browser_set_buttons(app: tauri::AppHandle, session: Option<String>, buttons: Vec<Button>) -> Result<(), String> {
    set_buttons(&app, &session_name(session)?, buttons)
}

#[tauri::command]
pub async fn browser_current(app: tauri::AppHandle, session: Option<String>) -> Result<Option<Page>, String> {
    Ok(current_page(&app, &session_name(session)?))
}

fn open_page(app: &tauri::AppHandle, session: &str) -> Option<tauri::Webview> {
    let state = app.state::<BrowserState>();
    let sessions = state.sessions.lock().expect("browser mutex poisoned");
    app.get_webview(&page_label(&sessions.get(session)?.label))
}

// Waits on a blocking thread: the result arrives on the main thread.
async fn wait_for<T: Send + 'static>(rx: mpsc::Receiver<T>, timeout: Duration) -> Result<T, mpsc::RecvTimeoutError> {
    tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(timeout))
        .await
        .unwrap_or(Err(mpsc::RecvTimeoutError::Disconnected))
}

#[tauri::command]
pub async fn browser_read(app: tauri::AppHandle, session: Option<String>) -> Result<core::PageContent, String> {
    let session = session_name(session)?;
    let page = open_page(&app, &session).ok_or_else(|| not_open(&session))?;
    let (tx, rx) = mpsc::channel();
    page.eval_with_callback(core::READ_SCRIPT, move |result| {
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    // wry drops the callback of a script queued before the first load
    // finishes, which disconnects the channel.
    match wait_for(rx, READ_TIMEOUT).await {
        Ok(result) => core::page_content(&result).map_err(to_browser_command_error),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err("UNAVAILABLE: the page is still loading".to_string()),
        Err(mpsc::RecvTimeoutError::Timeout) => Err("UNAVAILABLE: the page didn't answer in time".to_string()),
    }
}

fn session_cookie(cookie: &tauri::webview::Cookie<'static>) -> core::SessionCookie {
    core::SessionCookie {
        name: cookie.name().to_string(),
        value: cookie.value().to_string(),
        domain: cookie.domain().map(str::to_string),
        path: cookie.path().map(str::to_string),
        secure: cookie.secure().unwrap_or(false),
        expires_unix: cookie.expires_datetime().map(|at| at.unix_timestamp()),
    }
}

// The open page's cookies, or a hidden webview's on the same store when
// the window is closed — WebView2 only has a cookie manager on a live
// webview. Async: WebView2 deadlocks reading cookies synchronously.
async fn session_cookies(app: &tauri::AppHandle, session: &str) -> Result<Vec<core::SessionCookie>, String> {
    let read = |webview: tauri::Webview| async move {
        tauri::async_runtime::spawn_blocking(move || webview.cookies())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("UNAVAILABLE: couldn't read the session: {e}"))
    };
    if let Some(page) = open_page(app, session) {
        return Ok(read(page).await?.iter().map(session_cookie).collect());
    }
    let label = format!("{}-cookies", core::session_label(session));
    let (tx, rx) = mpsc::channel();
    let builder = tauri::WebviewWindowBuilder::new(app, &label, WebviewUrl::External("about:blank".parse().expect("valid URL")))
        .visible(false)
        .inner_size(1.0, 1.0)
        .on_page_load(move |_, payload| {
            if payload.event() == PageLoadEvent::Finished {
                let _ = tx.send(());
            }
        });
    #[cfg(target_os = "macos")]
    let builder = builder.data_store_identifier(core::session_key(session));
    #[cfg(not(target_os = "macos"))]
    let builder = builder.data_directory(core::session_data_dir(
        &app.path().app_local_data_dir().map_err(|e| format!("UNAVAILABLE: {e}"))?,
        session,
    ));
    let hidden = builder.build().map_err(|e| format!("UNAVAILABLE: couldn't open the session: {e}"))?;
    let _ = wait_for(rx, COOKIE_PAGE_TIMEOUT).await;
    let webview = hidden.as_ref().clone();
    remember_user_agent(app, &webview);
    let cookies = read(webview).await;
    let _ = hidden.destroy();
    Ok(cookies?.iter().map(session_cookie).collect())
}

#[tauri::command]
pub async fn browser_fetch(
    app: tauri::AppHandle,
    url: String,
    session: Option<String>,
    max_bytes: Option<u64>,
    timeout_ms: Option<u64>,
) -> Result<tauri::ipc::Response, String> {
    let session = session_name(session)?;
    core::web_url(&url).map_err(to_browser_command_error)?;
    let cookies = session_cookies(&app, &session).await?;
    let user_agent = app.state::<BrowserState>().user_agent.lock().expect("browser mutex poisoned").clone();
    core::fetch(core::SessionFetch { url: &url, cookies: &cookies, user_agent: user_agent.as_deref(), timeout_ms, max_bytes })
        .await
        .map(tauri::ipc::Response::new)
        .map_err(crate::to_http_command_error)
}

#[tauri::command]
pub async fn browser_clear_session(app: tauri::AppHandle, session: Option<String>) -> Result<(), String> {
    if !supported() {
        return Err("UNSUPPORTED: the browser window needs macOS 14 or later, or Windows".to_string());
    }
    let session = session_name(session)?;
    clear_store(&app, &session).await?;
    if let Some(page) = open_page(&app, &session) {
        let _ = page.reload();
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn clear_store(app: &tauri::AppHandle, session: &str) -> Result<(), String> {
    let key = core::session_key(session);
    let (tx, rx) = mpsc::channel();
    let started = tx.clone();
    app.run_on_main_thread(move || {
        let done = tx.clone();
        if let Err(e) = core::clear_store(key, move || {
            let _ = done.send(Ok(()));
        }) {
            let _ = started.send(Err(to_browser_command_error(e)));
        }
    })
    .map_err(|e| e.to_string())?;
    match wait_for(rx, Duration::from_secs(30)).await {
        Ok(result) => result,
        Err(_) => Err("UNAVAILABLE: clearing the session didn't finish".to_string()),
    }
}

#[cfg(not(target_os = "macos"))]
async fn clear_store(app: &tauri::AppHandle, session: &str) -> Result<(), String> {
    if let Some(page) = open_page(app, session) {
        return page.clear_all_browsing_data().map_err(|e| format!("UNAVAILABLE: {e}"));
    }
    let dir = core::session_data_dir(&app.path().app_local_data_dir().map_err(|e| format!("UNAVAILABLE: {e}"))?, session);
    // WebView2 can hold the folder for a moment after the window closes.
    for _ in 0..10 {
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => sleep(Duration::from_millis(500)).await,
        }
    }
    Err("UNAVAILABLE: the session is still in use; try again in a moment".to_string())
}

#[cfg(not(target_os = "macos"))]
async fn sleep(duration: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(duration)).await;
}

// The `chain-browser` URI scheme: the toolbar's page, its state and its
// actions. Registered on every webview, so state and actions answer only
// the session's own toolbar — the page can't fake a button press.
pub fn protocol(
    ctx: tauri::UriSchemeContext<'_, tauri::Wry>,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Cow<'static, [u8]>> {
    let respond = |status: u16, content_type: &str, body: Cow<'static, [u8]>| {
        tauri::http::Response::builder()
            .status(status)
            .header(tauri::http::header::CONTENT_TYPE, content_type)
            .body(body)
            .expect("valid response")
    };
    let path = request.uri().path();
    if path == "/toolbar" {
        return respond(200, "text/html; charset=utf-8", Cow::Borrowed(core::TOOLBAR_HTML.as_bytes()));
    }
    let app = ctx.app_handle();
    let found = {
        let state = app.state::<BrowserState>();
        let sessions = state.sessions.lock().expect("browser mutex poisoned");
        sessions
            .iter()
            .find(|(_, open)| toolbar_label(&open.label) == ctx.webview_label())
            .map(|(name, open)| (name.clone(), open.label.clone(), open.buttons.clone()))
    };
    let Some((session, label, buttons)) = found else {
        return respond(403, "text/plain", Cow::Borrowed(b"forbidden"));
    };
    let query = request.uri().query().map(url_query).unwrap_or_default();
    match path {
        "/state" => {
            let url = current_page(app, &session).map(|page| page.url).unwrap_or_default();
            let history = !cfg!(target_os = "macos");
            refresh_history(app, &label);
            let state = serde_json::json!({ "url": url, "buttons": buttons, "canGoBack": history, "canGoForward": history });
            respond(200, "application/json", Cow::Owned(state.to_string().into_bytes()))
        }
        "/action" => {
            toolbar_action(app, &session, &label, &buttons, &query);
            respond(204, "text/plain", Cow::Borrowed(b""))
        }
        _ => respond(404, "text/plain", Cow::Borrowed(b"not found")),
    }
}

fn url_query(query: &str) -> HashMap<String, String> {
    Url::parse(&format!("chain-browser://localhost/?{query}"))
        .map(|url| url.query_pairs().into_owned().collect())
        .unwrap_or_default()
}

fn toolbar_action(app: &tauri::AppHandle, session: &str, label: &str, buttons: &[Button], query: &HashMap<String, String>) {
    let Some(page) = app.get_webview(&page_label(label)) else { return };
    match query.get("do").map(String::as_str) {
        Some(action @ ("back" | "forward")) => go(&page, action == "back"),
        Some("reload") => {
            let _ = page.reload();
        }
        Some("button") => {
            let Some(id) = query.get("id") else { return };
            if !buttons.iter().any(|button| &button.id == id && button.enabled) {
                return;
            }
            if let Some(current) = current_page(app, session) {
                let _ = app.emit(
                    "chain://browser-button",
                    ButtonPayload { session: current.session, id: id.clone(), url: current.url, title: current.title },
                );
            }
        }
        _ => {}
    }
}

fn go(page: &tauri::Webview, back: bool) {
    #[cfg(target_os = "macos")]
    let _ = page.with_webview(move |webview| core::go(webview.inner(), back));
    #[cfg(not(target_os = "macos"))]
    let _ = page.eval(if back { "history.back()" } else { "history.forward()" });
}
