// Bridges the terminal capability contract (capabilities/terminal in
// chain-sdk): programs in a pseudo-terminal, owned by Chain Core so they
// keep running and buffering output while the page reloads. Sessions,
// sequence numbers and the backlog are chain_core::terminal; this file
// only reaches the webview. See agent-docs/capabilities/terminal/.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

use chain_core::terminal::{self as core, Backlog, SessionInfo, StartOptions, TerminalError, Terminals};
use tauri::{Emitter, Manager, Runtime};

#[derive(Default)]
pub struct TerminalState(OnceLock<Arc<Terminals>>);

// The SDK maps these prefixes onto ChainErrorCode — see packages/sdk/src/terminal.ts.
fn to_terminal_command_error(e: TerminalError) -> String {
    match e {
        TerminalError::InvalidArgument(m) => format!("INVALID_ARGUMENT: {m}"),
        TerminalError::NotFound(m) => format!("NOT_FOUND: {m}"),
        TerminalError::PermissionDenied(m) => format!("PERMISSION_DENIED: {m}"),
        TerminalError::Unavailable(m) => format!("UNAVAILABLE: {m}"),
        TerminalError::TimedOut(m) => format!("TIMEOUT: {m}"),
        TerminalError::Other(m) => m,
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExitPayload {
    session_id: String,
    exit: core::SessionExit,
}

pub fn terminals<R: Runtime>(app: &tauri::AppHandle<R>) -> Arc<Terminals> {
    let state = app.state::<TerminalState>();
    let terminals = state.0.get_or_init(|| {
        let (for_output, for_exit) = (app.clone(), app.clone());
        Arc::new(Terminals::new(
            Arc::new(move |chunk| {
                let _ = for_output.emit("chain://terminal-output", chunk);
            }),
            Arc::new(move |session_id, exit| {
                let payload = ExitPayload { session_id: session_id.to_string(), exit: exit.clone() };
                let _ = for_exit.emit("chain://terminal-exit", payload);
            }),
        ))
    });
    Arc::clone(terminals)
}

// Blocking work (the first spawn resolves the login-shell PATH, a paste
// into a program that isn't reading, a kill's grace period) stays off the
// main thread, where sync commands run.
async fn off_main<T: Send + 'static>(
    app: &tauri::AppHandle,
    work: impl FnOnce(&Terminals) -> Result<T, TerminalError> + Send + 'static,
) -> Result<T, String> {
    let terminals = terminals(app);
    tauri::async_runtime::spawn_blocking(move || work(&terminals))
        .await
        .map_err(|e| e.to_string())?
        .map_err(to_terminal_command_error)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartArgs {
    command: String,
    args: Option<Vec<String>>,
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
    cols: Option<u16>,
    rows: Option<u16>,
    label: Option<String>,
    metadata: Option<BTreeMap<String, String>>,
    backlog_bytes: Option<usize>,
}

#[tauri::command]
pub async fn terminal_start(app: tauri::AppHandle, options: StartArgs) -> Result<SessionInfo, String> {
    // Must be an existing folder inside a grant — see folders/CONTRACT.md.
    let cwd = match options.cwd {
        Some(cwd) => Some(
            crate::folders::folders(&app)?.working_directory(&cwd).map_err(crate::folders::to_folders_command_error)?,
        ),
        None => None,
    };
    let start = StartOptions {
        command: options.command,
        args: options.args.unwrap_or_default(),
        cwd,
        env: options.env.unwrap_or_default().into_iter().collect(),
        cols: options.cols.unwrap_or(80),
        rows: options.rows.unwrap_or(24),
        label: options.label.unwrap_or_default(),
        metadata: options.metadata.unwrap_or_default(),
        backlog_bytes: options.backlog_bytes.unwrap_or(core::DEFAULT_BACKLOG_BYTES),
    };
    off_main(&app, move |t| t.start(start)).await
}

#[tauri::command]
pub fn terminal_list(app: tauri::AppHandle) -> Vec<SessionInfo> {
    terminals(&app).list()
}

#[tauri::command]
pub fn terminal_backlog(app: tauri::AppHandle, session_id: String) -> Result<Backlog, String> {
    terminals(&app).backlog(&session_id).map_err(to_terminal_command_error)
}

#[tauri::command]
pub async fn terminal_write(app: tauri::AppHandle, session_id: String, data: String) -> Result<(), String> {
    off_main(&app, move |t| t.write(&session_id, &data)).await
}

#[tauri::command]
pub fn terminal_resize(app: tauri::AppHandle, session_id: String, cols: u16, rows: u16) -> Result<(), String> {
    terminals(&app).resize(&session_id, cols, rows).map_err(to_terminal_command_error)
}

#[tauri::command]
pub async fn terminal_kill(app: tauri::AppHandle, session_id: String) -> Result<(), String> {
    off_main(&app, move |t| t.kill(&session_id)).await
}

#[tauri::command]
pub fn terminal_remove(app: tauri::AppHandle, session_id: String) {
    terminals(&app).remove(&session_id);
}
