// Bridges the attention capability contract (capabilities/attention in
// chain-sdk): window focus, system notifications whose click brings the
// window back and names the notification, and a Dock bounce. Notifications
// are chain_core::attention; focus and the bounce are Tauri's own window
// methods. See agent-docs/capabilities/attention/.

use std::collections::HashMap;
use std::sync::Mutex;

use chain_core::attention::{self as core, Notified, Permission};
use tauri::{Emitter, Manager, Runtime};

#[derive(Default)]
pub struct AttentionState {
    // Which window showed each notification, so its click brings that one back.
    shown_by: Mutex<HashMap<String, String>>,
}

#[derive(Clone, serde::Serialize)]
struct ClickPayload {
    id: String,
}

/// At launch, so a click that activates the app is delivered too.
pub fn setup(app: &tauri::App) {
    let handle = app.handle().clone();
    core::start(Box::new(move |id| {
        let label = handle.state::<AttentionState>().shown_by.lock().expect("attention mutex poisoned").get(&id).cloned();
        let window = label
            .and_then(|label| handle.get_webview_window(&label))
            .or_else(|| handle.webview_windows().into_values().next());
        if let Some(window) = window {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            let _ = window.emit_to(tauri::EventTarget::webview(window.label()), "chain://attention-click", ClickPayload { id });
        }
    }));
}

pub fn on_window_event<R: Runtime>(window: &tauri::Window<R>, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::Focused(focused) = event {
        let _ = window.emit_to(tauri::EventTarget::webview(window.label()), "chain://attention-focus", *focused);
    }
}

#[tauri::command]
pub fn attention_is_focused(window: tauri::Window) -> Result<bool, String> {
    window.is_focused().map_err(|e| e.to_string())
}

// Once on macOS: `informational`, not `critical` (which bounces until the app is activated).
#[tauri::command]
pub fn attention_request(window: tauri::Window) -> Result<(), String> {
    window.request_user_attention(Some(tauri::UserAttentionType::Informational)).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn attention_permission() -> Result<Permission, String> {
    tauri::async_runtime::spawn_blocking(core::permission).await.map_err(|e| e.to_string())
}

// Off the main thread: the first call waits for the user to answer the
// permission prompt.
#[tauri::command]
pub async fn attention_notify(
    window: tauri::Window,
    state: tauri::State<'_, AttentionState>,
    id: String,
    title: String,
    body: Option<String>,
) -> Result<Notified, String> {
    if id.is_empty() || title.is_empty() {
        return Err("INVALID_ARGUMENT: a notification needs an id and a title".to_string());
    }
    state.shown_by.lock().expect("attention mutex poisoned").insert(id.clone(), window.label().to_string());
    let body = body.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || core::notify(&id, &title, &body)).await.map_err(|e| e.to_string())
}
