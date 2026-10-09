// Bridges the window capability contract (capabilities/window in
// chain-sdk): the app window's chrome, its insets and full-screen state.
// The startup options are package.json's "chain.window", compiled in
// (build.rs points CHAIN_PACKAGE_JSON at it) and applied in setup, before
// the first frame — including when the window first shows (showWhen),
// which keeps it undrawn until then. Options, defaults and the AppKit work are
// chain_core::window. See agent-docs/capabilities/window/.

use std::collections::HashMap;
use std::sync::{mpsc, Mutex};

use chain_core::window::{self as core, Appearance, Chrome, FirstShow, Insets, ResolvedOptions, ShowWhen};
use tauri::{Emitter, Manager, Runtime, Theme, WindowEvent};

const PACKAGE_JSON: &str = include_str!(env!("CHAIN_PACKAGE_JSON"));

#[derive(Default)]
pub struct WindowState {
    windows: Mutex<HashMap<String, Tracked>>,
}

#[derive(Default)]
struct Tracked {
    chrome: Chrome,
    insets: Insets,
    full_screen: bool,
    /// What the window waits for before its first appearance; None once it has appeared.
    awaiting: Option<ShowWhen>,
}

fn is_app_window(label: &str) -> bool {
    !label.starts_with(chain_core::browser::LABEL_PREFIX)
}

/// Runs `f` on the main thread — right away when already there, which is
/// where sync commands and window events run.
fn on_main<R: Runtime, T: Send + 'static>(
    window: &tauri::Window<R>,
    f: impl FnOnce(&tauri::Window<R>) -> T + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = mpsc::channel();
    let target = window.clone();
    window
        .run_on_main_thread(move || {
            let _ = tx.send(f(&target));
        })
        .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())
}

fn theme(appearance: Appearance) -> Option<Theme> {
    match appearance {
        Appearance::System => None,
        Appearance::Light => Some(Theme::Light),
        Appearance::Dark => Some(Theme::Dark),
    }
}

fn apply<R: Runtime>(window: &tauri::Window<R>, chrome: &Chrome, previous: &Chrome) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let ns_window = window.ns_window().map_err(|e| e.to_string())?;
        core::apply(ns_window, chrome);
    }
    #[cfg(not(target_os = "macos"))]
    if chrome.effective_style() != previous.effective_style() {
        let decorated = chrome.effective_style() != core::TitleBarStyle::Hidden;
        window.set_decorations(decorated).map_err(|e| e.to_string())?;
    }
    if chrome.appearance != previous.appearance {
        window.set_theme(theme(chrome.appearance)).map_err(|e| e.to_string())?;
    }
    if chrome.background_color != previous.background_color {
        let color = chrome.background_color.map(|[r, g, b, a]| tauri::window::Color(r, g, b, a));
        window.set_background_color(color).map_err(|e| e.to_string())?;
        for webview in window.webviews() {
            webview.set_background_color(color).map_err(|e| e.to_string())?;
            #[cfg(target_os = "macos")]
            {
                let draws = color.is_none();
                webview
                    .with_webview(move |page| core::page_draws_background(page.inner(), draws))
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

fn measure<R: Runtime>(window: &tauri::Window<R>, chrome: &Chrome) -> Insets {
    #[cfg(target_os = "macos")]
    return window.ns_window().map_or(Insets::default(), |ns_window| core::insets(ns_window, chrome));
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, chrome);
        Insets::default()
    }
}

/// Tells the window's page about new insets or full-screen state.
fn report<R: Runtime>(window: &tauri::Window<R>, tracked: &mut Tracked) {
    let insets = measure(window, &tracked.chrome);
    let full_screen = window.is_fullscreen().unwrap_or(false);
    if full_screen != tracked.full_screen {
        tracked.full_screen = full_screen;
        let _ = window.emit_to(window.label(), "chain://window-full-screen", full_screen);
    }
    if insets != tracked.insets {
        tracked.insets = insets;
        let _ = window.emit_to(window.label(), "chain://window-insets", insets);
    }
}

fn set_undrawn<R: Runtime>(window: &tauri::Window<R>, undrawn: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    core::set_undrawn(window.ns_window().map_err(|e| e.to_string())?, undrawn);
    #[cfg(not(target_os = "macos"))]
    let _ = (window, undrawn);
    Ok(())
}

/// The window's first appearance; nothing once it has appeared. The lock
/// isn't held while AppKit changes the window.
fn show<R: Runtime>(window: &tauri::Window<R>) -> Result<(), String> {
    on_main(window, |window| {
        let state = window.state::<WindowState>();
        let awaiting = state.windows.lock().expect("window state poisoned").get_mut(window.label()).and_then(|t| t.awaiting.take());
        if awaiting.is_none() {
            return Ok(());
        }
        set_undrawn(window, false)?;
        let _ = window.emit_to(window.label(), "chain://window-shown", ());
        Ok(())
    })?
}

/// Applies package.json's "chain.window" to the windows Tauri created
/// from tauri.conf.json. Without options it changes nothing at all. Setup
/// runs before the first frame, so a window kept undrawn never flashes.
pub fn setup<R: Runtime>(app: &tauri::App<R>) -> Result<(), Box<dyn std::error::Error>> {
    let chrome = Chrome::from_package_json(PACKAGE_JSON).map_err(|e| e.0)?;
    let first_show = FirstShow::from_package_json(PACKAGE_JSON).map_err(|e| e.0)?;
    let awaiting = Some(first_show.effective_when()).filter(|when| *when != ShowWhen::Immediately);
    for (label, window) in app.windows() {
        if !is_app_window(&label) {
            continue;
        }
        if chrome != Chrome::default() {
            apply(&window, &chrome, &Chrome::default())?;
        }
        if awaiting.is_some() {
            set_undrawn(&window, true)?;
            let window = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(first_show.timeout);
                let _ = show(&window);
            });
        }
        let tracked = Tracked {
            insets: measure(&window, &chrome),
            full_screen: window.is_fullscreen().unwrap_or(false),
            chrome: chrome.clone(),
            awaiting,
        };
        app.state::<WindowState>().windows.lock().expect("window state poisoned").insert(label, tracked);
    }
    Ok(())
}

/// Insets change with full screen, and with a style or size change (which
/// resizes the bar).
pub fn on_window_event<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    if !matches!(
        event,
        WindowEvent::Resized(_) | WindowEvent::Focused(_) | WindowEvent::ScaleFactorChanged { .. } | WindowEvent::ThemeChanged(_)
    ) {
        return;
    }
    let state = window.state::<WindowState>();
    let mut windows = state.windows.lock().expect("window state poisoned");
    let Some(tracked) = windows.get_mut(window.label()) else { return };
    report(window, tracked);
}

#[tauri::command]
pub fn window_availability() -> core::Availability {
    core::availability()
}

#[tauri::command]
pub fn window_options(window: tauri::Window, state: tauri::State<WindowState>) -> ResolvedOptions {
    let windows = state.windows.lock().expect("window state poisoned");
    windows.get(window.label()).map(|t| t.chrome.clone()).unwrap_or_default().resolved()
}

// The lock isn't held while AppKit changes the window: that can send
// window events (on_window_event) straight away.
#[tauri::command]
pub fn window_set_options(window: tauri::Window, options: serde_json::Value) -> Result<(), String> {
    on_main(&window, move |window| {
        let state = window.state::<WindowState>();
        let previous = state.windows.lock().expect("window state poisoned").get(window.label()).map(|t| t.chrome.clone());
        let previous = previous.unwrap_or_default();
        let mut chrome = previous.clone();
        chrome.merge(options).map_err(|e| format!("INVALID_ARGUMENT: {}", e.0))?;
        apply(window, &chrome, &previous)?;
        let mut windows = state.windows.lock().expect("window state poisoned");
        let tracked = windows.entry(window.label().to_string()).or_default();
        tracked.chrome = chrome;
        report(window, tracked);
        Ok(())
    })?
}

#[tauri::command]
pub fn window_insets(window: tauri::Window, state: tauri::State<WindowState>) -> Insets {
    let windows = state.windows.lock().expect("window state poisoned");
    windows.get(window.label()).map(|t| t.insets).unwrap_or_default()
}

#[tauri::command]
pub fn window_is_full_screen(window: tauri::Window) -> Result<bool, String> {
    window.is_fullscreen().map_err(|e| e.to_string())
}

/// The page's drag regions. On macOS a native view over the page takes
/// the presses there (`true`); elsewhere the SDK listens for them itself
/// and calls window_start_drag (`false`).
#[tauri::command]
pub fn window_set_drag_regions(webview: tauri::Webview, regions: Vec<core::Rect>, holes: Vec<core::Rect>) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        webview
            .with_webview(move |page| core::set_drag_regions(page.inner(), &regions, &holes))
            .map_err(|e| e.to_string())?;
        Ok(true)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (webview, regions, holes);
        Ok(false)
    }
}

#[tauri::command]
pub fn window_show(window: tauri::Window) -> Result<(), String> {
    show(&window)
}

#[tauri::command]
pub fn window_is_shown(window: tauri::Window, state: tauri::State<WindowState>) -> bool {
    let windows = state.windows.lock().expect("window state poisoned");
    windows.get(window.label()).is_none_or(|t| t.awaiting.is_none())
}

/// The SDK, once its page has drawn a frame — on every load, reloads too.
#[tauri::command]
pub fn window_page_painted(window: tauri::Window, state: tauri::State<WindowState>) -> Result<(), String> {
    let awaiting = state.windows.lock().expect("window state poisoned").get(window.label()).and_then(|t| t.awaiting);
    if awaiting == Some(ShowWhen::FirstPaint) {
        show(&window)?;
    }
    Ok(())
}

#[tauri::command]
pub fn window_start_drag(window: tauri::Window) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

/// Double-click on a drag region: what the OS's own title bar would do.
#[tauri::command]
pub fn window_title_bar_double_click(window: tauri::Window) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return on_main(&window, |window| {
        if let Ok(ns_window) = window.ns_window() {
            core::title_bar_double_click(ns_window);
        }
    });
    #[cfg(not(target_os = "macos"))]
    {
        let result = if window.is_maximized().map_err(|e| e.to_string())? {
            window.unmaximize()
        } else {
            window.maximize()
        };
        result.map_err(|e| e.to_string())
    }
}
