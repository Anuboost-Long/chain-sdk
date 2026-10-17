// Bridges the browser-view capability contract (capabilities/browser-view
// in chain-sdk): other sites shown inside the app's window. The views are
// the OS's own web views, made and driven by chain_core::browser_view on
// the main thread — never Tauri webviews, so they never get the app's IPC.
// This file routes commands there, emits the views' events, closes a page's
// views when it reloads or its window closes, and times out held requests.
// See agent-docs/capabilities/browser-view/.

use std::collections::HashMap;
use std::sync::{mpsc, Mutex};

use chain_core::browser_view::{self as core, Bounds, BrowserViewError, Event};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub struct BrowserViewState {
    // View id → (label of the app page that created it, its window's label).
    owners: Mutex<HashMap<String, (String, String)>>,
}

// The SDK maps these prefixes onto ChainErrorCode — see packages/sdk/src/browser-view.ts.
fn to_browser_view_command_error(e: BrowserViewError) -> String {
    match e {
        BrowserViewError::InvalidArgument(m) => format!("INVALID_ARGUMENT: {m}"),
        BrowserViewError::NotFound(m) => format!("NOT_FOUND: {m}"),
        BrowserViewError::Unsupported(m) => format!("UNSUPPORTED: {m}"),
        BrowserViewError::Other(m) => m,
    }
}

/// Routes every view's events to the app, and holds' timeouts back to the main thread.
pub fn setup(app: &tauri::App) {
    let app = app.handle().clone();
    core::set_sink(move |event| {
        let _ = match event {
            Event::State(state) => app.emit("chain://browser-view-state", state),
            Event::LoadFailed(failure) => app.emit("chain://browser-view-load-failed", failure),
            Event::Crash(crash) => app.emit("chain://browser-view-crash", crash),
            Event::NewWindow(request) => app.emit("chain://browser-view-new-window", request),
            Event::NavigationRequest(request) => {
                let request_id = request.request_id.clone();
                after_hold(&app, move || core::answer(&request_id, false));
                app.emit("chain://browser-view-navigation-request", request)
            }
            Event::HeldWindow { held_id } => {
                after_hold(&app, move || core::drop_held(&held_id));
                Ok(())
            }
            Event::Message(message) => {
                let request_id = message.request_id.clone();
                after(&app, core::ASK_TIMEOUT, move || core::reply(&request_id, Err("the app didn't answer in time".to_string())));
                app.emit("chain://browser-view-message", message)
            }
        };
    });
}

fn after_hold(app: &tauri::AppHandle, f: impl FnOnce() + Send + 'static) {
    after(app, core::HOLD_TIMEOUT, f);
}

fn after(app: &tauri::AppHandle, delay: std::time::Duration, f: impl FnOnce() + Send + 'static) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        let _ = app.run_on_main_thread(f);
    });
}

// Runs on the main thread, where WebKit's objects live; waits off it.
async fn on_main<T: Send + 'static>(
    app: &tauri::AppHandle,
    f: impl FnOnce() -> Result<T, BrowserViewError> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })
    .map_err(|e| e.to_string())?;
    wait(rx).await
}

// For native calls that finish later, on the main thread, through `done`.
async fn on_main_until_done<T: Send + 'static>(
    app: &tauri::AppHandle,
    f: impl FnOnce(Box<dyn FnOnce(Result<T, BrowserViewError>)>) + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        f(Box::new(move |result| {
            let _ = tx.send(result);
        }))
    })
    .map_err(|e| e.to_string())?;
    wait(rx).await
}

async fn wait<T: Send + 'static>(rx: mpsc::Receiver<Result<T, BrowserViewError>>) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || rx.recv())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|_| "the view didn't answer".to_string())?
        .map_err(to_browser_view_command_error)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    available: bool,
    load_failure_reasons: bool,
    crashes: bool,
    decide_navigation: bool,
    input_details: bool,
    input_age: bool,
    fullscreen: bool,
    swipe_navigation: bool,
    inspectable: bool,
    picture_in_picture: bool,
    rules: bool,
    page_scripts: bool,
    run_in_frames: bool,
}

#[tauri::command]
pub fn browser_view_availability() -> Availability {
    let all = core::supported();
    Availability {
        available: all,
        load_failure_reasons: all,
        crashes: all,
        decide_navigation: all,
        input_details: all,
        input_age: all,
        fullscreen: all,
        swipe_navigation: all,
        inspectable: all,
        picture_in_picture: core::picture_in_picture_supported(),
        rules: all,
        page_scripts: all,
        run_in_frames: all,
    }
}

fn session_name(session: &str) -> Result<String, String> {
    chain_core::browser::session_name(Some(session)).map_err(|e| match e {
        chain_core::browser::BrowserError::InvalidArgument(m) => format!("INVALID_ARGUMENT: {m}"),
        other => format!("{other:?}"),
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOptions {
    url: String,
    session: String,
    bounds: Bounds,
    visible: Option<bool>,
    decide_navigation: Option<bool>,
    allow_fullscreen: Option<bool>,
    allow_picture_in_picture: Option<bool>,
    swipe_navigation: Option<bool>,
    inspectable: Option<bool>,
}

#[tauri::command]
pub async fn browser_view_create(app: tauri::AppHandle, webview: tauri::Webview, options: CreateOptions) -> Result<String, String> {
    if !core::supported() {
        return Err("UNSUPPORTED: embedded views need macOS 14 or later today".to_string());
    }
    let app_origin = app.config().build.dev_url.clone();
    core::view_url(&options.url, app_origin.as_ref()).map_err(to_browser_view_command_error)?;
    let session = session_name(&options.session)?;
    let bounds = options.bounds.checked().map_err(to_browser_view_command_error)?;
    let (tx, rx) = mpsc::channel();
    #[cfg(target_os = "macos")]
    webview
        .with_webview(move |page| {
            let _ = tx.send(core::create(core::Create {
                page: page.inner(),
                session_key: chain_core::browser::session_key(&session),
                session: &session,
                url: &options.url,
                bounds,
                visible: options.visible.unwrap_or(true),
                app_origin,
                decide_navigation: options.decide_navigation.unwrap_or(false),
                allow_fullscreen: options.allow_fullscreen.unwrap_or(false),
                allow_picture_in_picture: options.allow_picture_in_picture.unwrap_or(false),
                swipe_navigation: options.swipe_navigation.unwrap_or(false),
                inspectable: options.inspectable.unwrap_or(false),
            }));
        })
        .map_err(|e| e.to_string())?;
    #[cfg(not(target_os = "macos"))]
    let _ = (&tx, session, bounds, app_origin);
    let id = wait(rx).await?;
    let owner = (webview.label().to_string(), webview.window().label().to_string());
    app.state::<BrowserViewState>().owners.lock().expect("browser-view mutex poisoned").insert(id.clone(), owner);
    Ok(id)
}

fn close_where(app: &tauri::AppHandle, owned: impl Fn(&(String, String)) -> bool) {
    let closing: Vec<String> = {
        let state = app.state::<BrowserViewState>();
        let mut owners = state.owners.lock().expect("browser-view mutex poisoned");
        let closing = owners.iter().filter(|(_, owner)| owned(owner)).map(|(id, _)| id.clone()).collect();
        owners.retain(|_, owner| !owned(owner));
        closing
    };
    if !closing.is_empty() {
        let _ = app.run_on_main_thread(move || closing.iter().for_each(|id| core::close(id)));
    }
}

/// A page that reloads or navigates away can't see the views it made.
pub fn release_page(webview: &tauri::Webview) {
    let label = webview.label().to_string();
    close_where(webview.app_handle(), |(page, _)| page == &label);
}

pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::Destroyed = event {
        let label = window.label().to_string();
        close_where(window.app_handle(), |(_, owner_window)| owner_window == &label);
    }
}

#[tauri::command]
pub async fn browser_view_close(app: tauri::AppHandle, id: String) -> Result<(), String> {
    app.state::<BrowserViewState>().owners.lock().expect("browser-view mutex poisoned").remove(&id);
    on_main(&app, move || {
        core::close(&id);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn browser_view_set_bounds(app: tauri::AppHandle, id: String, bounds: Bounds) -> Result<(), String> {
    let bounds = bounds.checked().map_err(to_browser_view_command_error)?;
    on_main(&app, move || core::set_bounds(&id, bounds)).await
}

#[tauri::command]
pub async fn browser_view_set_visible(app: tauri::AppHandle, id: String, visible: bool) -> Result<(), String> {
    on_main(&app, move || core::set_visible(&id, visible)).await
}

#[tauri::command]
pub async fn browser_view_load(app: tauri::AppHandle, id: String, url: String) -> Result<(), String> {
    let app_origin = app.config().build.dev_url.clone();
    core::view_url(&url, app_origin.as_ref()).map_err(to_browser_view_command_error)?;
    on_main(&app, move || core::navigate(&id, &url)).await
}

#[tauri::command]
pub async fn browser_view_go(app: tauri::AppHandle, id: String, back: bool) -> Result<(), String> {
    on_main(&app, move || core::go(&id, back)).await
}

#[tauri::command]
pub async fn browser_view_reload(app: tauri::AppHandle, id: String) -> Result<(), String> {
    on_main(&app, move || core::reload(&id)).await
}

#[tauri::command]
pub async fn browser_view_stop(app: tauri::AppHandle, id: String) -> Result<(), String> {
    on_main(&app, move || core::stop(&id)).await
}

#[tauri::command]
pub async fn browser_view_state(app: tauri::AppHandle, id: String) -> Result<core::State, String> {
    on_main(&app, move || core::state(&id)).await
}

#[tauri::command]
pub async fn browser_view_answer(app: tauri::AppHandle, request_id: String, allow: bool) -> Result<(), String> {
    on_main(&app, move || {
        core::answer(&request_id, allow);
        Ok(())
    })
    .await
}

fn list_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("INVALID_ARGUMENT: id is empty".to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_view_set_rules(app: tauri::AppHandle, session: String, id: String, rules: Vec<serde_json::Value>) -> Result<(), String> {
    let session = session_name(&session)?;
    list_id(&id)?;
    let rules = core::parse_rules(rules).map_err(to_browser_view_command_error)?;
    let rules_json = core::content_rules_json(&rules).map_err(to_browser_view_command_error)?;
    let session_key = chain_core::browser::session_key(&session);
    on_main_until_done(&app, move |done| core::set_rules(&session, session_key, &id, rules_json, done)).await
}

#[tauri::command]
pub async fn browser_view_remove_rules(app: tauri::AppHandle, session: String, id: String) -> Result<(), String> {
    let session = session_name(&session)?;
    let session_key = chain_core::browser::session_key(&session);
    on_main(&app, move || {
        core::remove_rules(&session, session_key, &id);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn browser_view_set_script(app: tauri::AppHandle, session: String, id: String, source: String) -> Result<(), String> {
    let session = session_name(&session)?;
    list_id(&id)?;
    on_main(&app, move || {
        core::set_script(&session, &id, &source);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn browser_view_remove_script(app: tauri::AppHandle, session: String, id: String) -> Result<(), String> {
    let session = session_name(&session)?;
    on_main(&app, move || {
        core::remove_script(&session, &id);
        Ok(())
    })
    .await
}

/// `value` is the app's answer as JSON; `error` says why there's none.
#[tauri::command]
pub async fn browser_view_reply(app: tauri::AppHandle, request_id: String, value: Option<String>, error: Option<String>) -> Result<(), String> {
    let answer = match (value, error) {
        (Some(json), None) => Ok(json),
        (_, error) => Err(error.unwrap_or_else(|| "no answer".to_string())),
    };
    on_main(&app, move || {
        core::reply(&request_id, answer);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn browser_view_run_in_frames(app: tauri::AppHandle, id: String, body: String) -> Result<Vec<core::FrameResult>, String> {
    on_main_until_done(&app, move |done| core::run_in_frames(&id, &body, done)).await
}
