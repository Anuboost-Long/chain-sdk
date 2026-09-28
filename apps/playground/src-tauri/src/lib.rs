use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use tauri::{Emitter, Manager};

mod dev_inspector;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

// Bridges the platform capability contract (capabilities/platform in
// chain-sdk) to the Chain SDK. Keep this thin — all the real logic
// lives in chain_core.
#[tauri::command]
fn get_platform_info() -> Result<chain_core::platform::PlatformInfo, String> {
    chain_core::platform::get_platform_info(tauri::VERSION)
        .map_err(|_| "UNSUPPORTED".to_string())
}

// Bridges the storage capability contract (capabilities/storage in
// chain-sdk). The database is opened lazily, on first use, into a single
// file in this app's per-user data directory — see CONTRACT.md.
struct StorageState(Mutex<Option<chain_core::storage::Database>>);

fn with_storage<T>(
    app: &tauri::AppHandle,
    state: &tauri::State<StorageState>,
    f: impl FnOnce(&chain_core::storage::Database) -> Result<T, chain_core::storage::StorageError>,
) -> Result<T, String> {
    let mut guard = state.0.lock().expect("storage mutex poisoned");
    if guard.is_none() {
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let db = chain_core::storage::Database::open(&dir.join("app.db")).map_err(|e| e.0)?;
        *guard = Some(db);
    }
    f(guard.as_ref().expect("just initialized above")).map_err(|e| e.0)
}

#[tauri::command]
fn storage_migrate(
    app: tauri::AppHandle,
    state: tauri::State<StorageState>,
    migrations: Vec<chain_core::storage::Migration>,
) -> Result<(), String> {
    with_storage(&app, &state, |db| db.migrate(&migrations))
}

#[tauri::command]
fn storage_query(
    app: tauri::AppHandle,
    state: tauri::State<StorageState>,
    sql: String,
    params: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    with_storage(&app, &state, |db| db.query(&sql, &params))
}

#[tauri::command]
fn storage_execute(
    app: tauri::AppHandle,
    state: tauri::State<StorageState>,
    sql: String,
    params: Vec<serde_json::Value>,
) -> Result<chain_core::storage::ExecuteResult, String> {
    with_storage(&app, &state, |db| db.execute(&sql, &params))
}

// Bridges the files capability contract (capabilities/files in
// chain-sdk). The managed directory is opened lazily, on first use, as a
// `files/` sibling of storage's `app.db` in this app's per-user data
// directory — see CONTRACT.md.
struct FilesState(Mutex<Option<chain_core::files::Files>>);

fn with_files<T>(
    app: &tauri::AppHandle,
    state: &tauri::State<FilesState>,
    f: impl FnOnce(&chain_core::files::Files) -> Result<T, chain_core::files::FilesError>,
) -> Result<T, String> {
    let mut guard = state.0.lock().expect("files mutex poisoned");
    if guard.is_none() {
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let files = chain_core::files::Files::open(&dir.join("files")).map_err(to_files_command_error)?;
        *guard = Some(files);
    }
    f(guard.as_ref().expect("just initialized above")).map_err(to_files_command_error)
}

// The SDK checks for this exact "NOT_FOUND: " prefix to map onto
// ChainErrorCode::NOT_FOUND — see packages/sdk/src/files.ts.
fn to_files_command_error(e: chain_core::files::FilesError) -> String {
    match e {
        chain_core::files::FilesError::NotFound(m) => format!("NOT_FOUND: {m}"),
        chain_core::files::FilesError::Other(m) => m,
    }
}

#[tauri::command]
fn files_write(
    app: tauri::AppHandle,
    state: tauri::State<FilesState>,
    bytes: Vec<u8>,
    extension: Option<String>,
) -> Result<String, String> {
    with_files(&app, &state, |files| files.write(&bytes, extension.as_deref()))
}

#[tauri::command]
fn files_read(
    app: tauri::AppHandle,
    state: tauri::State<FilesState>,
    reference: String,
) -> Result<Vec<u8>, String> {
    with_files(&app, &state, |files| files.read(&reference))
}

// Internal — not part of the public SDK contract. Resolves `reference`
// to an absolute path so the SDK's `url()` can feed it to Tauri's own
// `convertFileSrc`; the app itself never sees a real filesystem path.
#[tauri::command]
fn files_resolve_path(
    app: tauri::AppHandle,
    state: tauri::State<FilesState>,
    reference: String,
) -> Result<String, String> {
    with_files(&app, &state, |files| {
        files.resolve(&reference).map(|p| p.to_string_lossy().to_string())
    })
}

#[tauri::command]
fn files_delete(
    app: tauri::AppHandle,
    state: tauri::State<FilesState>,
    reference: String,
) -> Result<(), String> {
    with_files(&app, &state, |files| files.delete(&reference))
}

fn to_pick_command_error(e: chain_core::files::PickError) -> String {
    use chain_core::files::PickError::*;
    match e {
        InvalidArgument(m) => format!("INVALID_ARGUMENT: {m}"),
        Unavailable(m) => format!("UNAVAILABLE: {m}"),
        Other(m) => m,
    }
}

// Always shows the save panel (a sheet on macOS); native writes the bytes
// where the user chose. Returns the chosen file name, or null on cancel —
// never a path. See CONTRACT.md's save() section.
#[tauri::command]
async fn files_save(
    window: tauri::Window,
    bytes: Vec<u8>,
    suggested_name: Option<String>,
    extensions: Option<Vec<String>>,
) -> Result<Option<String>, String> {
    let options = chain_core::files::SaveOptions {
        suggested_name,
        extensions: extensions.unwrap_or_default(),
    };
    chain_core::files::save(&window, &bytes, &options).await.map_err(to_pick_command_error)
}

#[derive(serde::Serialize)]
struct PickedFileHeader {
    name: String,
    size: usize,
}

// Async so the picker (a sheet on macOS, attached to the calling window)
// never blocks the UI thread while it's open. Returns raw bytes rather
// than JSON: a u32 little-endian header length, a JSON header
// `[{ name, size }]`, then every file's bytes back to back — JSON number
// arrays would turn a 20 MB PDF into ~70 MB of text. The SDK's
// `files.pick()` decodes it (packages/sdk/src/files.ts).
#[tauri::command]
async fn files_pick(
    window: tauri::Window,
    multiple: Option<bool>,
    extensions: Option<Vec<String>>,
) -> Result<tauri::ipc::Response, String> {
    let options = chain_core::files::PickOptions {
        multiple: multiple.unwrap_or(false),
        extensions: extensions.unwrap_or_default(),
    };
    let picked = chain_core::files::pick(&window, &options).await.map_err(to_pick_command_error)?;

    let header: Vec<PickedFileHeader> = picked
        .iter()
        .map(|file| PickedFileHeader { name: file.name.clone(), size: file.bytes.len() })
        .collect();
    let header = serde_json::to_vec(&header).map_err(|e| e.to_string())?;
    let total: usize = picked.iter().map(|file| file.bytes.len()).sum();
    let mut body = Vec::with_capacity(4 + header.len() + total);
    body.extend_from_slice(&(header.len() as u32).to_le_bytes());
    body.extend_from_slice(&header);
    for file in &picked {
        body.extend_from_slice(&file.bytes);
    }
    Ok(tauri::ipc::Response::new(body))
}

// Bridges the http capability contract (capabilities/http in
// chain-sdk). Stateless — no data-directory involvement, unlike storage/
// files — each call is an independent request. Async, unlike the sync
// storage_*/files_* commands above: reqwest's async client just runs as
// a task on Tauri's own existing runtime, where reqwest::blocking would
// panic trying to start a nested runtime from within one — see
// agent-docs/capabilities/http/AGENTS.md.
#[tauri::command]
async fn http_get(url: String) -> Result<chain_core::http::HttpResponse, String> {
    chain_core::http::get(&url).await.map_err(to_http_command_error)
}

fn to_http_command_error(e: chain_core::http::HttpError) -> String {
    match e {
        chain_core::http::HttpError::InvalidUrl(m) => format!("INVALID_ARGUMENT: {m}"),
        chain_core::http::HttpError::Unavailable(m) => format!("UNAVAILABLE: {m}"),
        chain_core::http::HttpError::Other(m) => m,
    }
}

// Bridges the agent-server capability contract (capabilities/agent-server
// in chain-sdk). chain_core::agent_server owns the actual socket/HTTP
// parsing and knows nothing about Tauri; this is the "native listens, JS
// handles, native replies" plumbing that turns its `dispatch` callback
// into an event to the webview and a blocking wait for
// `__chain_agent_server_respond` to call back — the exact same shape
// dev_inspector.rs's `run_eval` already proves works in this app shell,
// generalized beyond raw `eval` and shipped unconditionally (not gated
// behind a Cargo feature) since this is a real capability, not a
// dev-only bridge. See agent-docs/capabilities/agent-server/CONTRACT.md.
#[derive(Default)]
struct AgentServerState {
    handle: Mutex<Option<chain_core::agent_server::ServerHandle>>,
    // Arc'd separately from `handle` so the dispatch closure (which
    // outlives the `agent_server_start` command call that created it) can
    // hold its own clone without needing the whole state to be `Clone`.
    pending: Arc<Mutex<Option<(u64, mpsc::Sender<AgentServerReply>)>>>,
    next_id: Arc<Mutex<u64>>,
}

struct AgentServerReply {
    ok: bool,
    status: Option<u16>,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
    error: Option<String>,
}

// A safety default (see CONTRACT.md's Errors on the 504 timeout path),
// not a contract parameter — bounds how long one slow/broken handler
// call can hold up the caller waiting on it. Longer than
// dev_inspector.rs's 10s since a real tool call (e.g. searching/writing
// app data) is expected to take longer than a debug `eval`.
const AGENT_SERVER_HANDLER_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentServerRequestPayload {
    id: u64,
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: String,
}

fn to_agent_server_command_error(e: chain_core::agent_server::AgentServerError) -> String {
    use chain_core::agent_server::AgentServerError::*;
    match e {
        InvalidPort(m) => format!("INVALID_ARGUMENT: {m}"),
        Unavailable(m) => format!("UNAVAILABLE: {m}"),
        PermissionDenied(m) => format!("PERMISSION_DENIED: {m}"),
        Other(m) => m,
    }
}

#[tauri::command]
fn agent_server_start(
    app: tauri::AppHandle,
    state: tauri::State<AgentServerState>,
    port: Option<i64>,
) -> Result<u16, String> {
    let mut handle_guard = state.handle.lock().expect("agent-server mutex poisoned");
    if handle_guard.is_some() {
        return Err("UNAVAILABLE: agent server is already running".to_string());
    }

    let pending = Arc::clone(&state.pending);
    let next_id = Arc::clone(&state.next_id);
    let app = app.clone();

    let dispatch = move |request: chain_core::agent_server::AgentServerRequest| {
        let id = {
            let mut guard = next_id.lock().expect("agent-server id mutex poisoned");
            *guard += 1;
            *guard
        };
        let (tx, rx) = mpsc::channel();
        *pending.lock().expect("agent-server pending mutex poisoned") = Some((id, tx));

        let payload = AgentServerRequestPayload {
            id,
            method: request.method,
            path: request.path,
            headers: request.headers,
            body: request.body,
        };
        if app.emit("chain://agent-server-request", payload).is_err() {
            *pending.lock().expect("agent-server pending mutex poisoned") = None;
            return chain_core::agent_server::DispatchOutcome::HandlerFailed(
                "failed to reach the app's registered handler".to_string(),
            );
        }

        match rx.recv_timeout(AGENT_SERVER_HANDLER_TIMEOUT) {
            Ok(reply) if reply.ok => {
                chain_core::agent_server::DispatchOutcome::Response(chain_core::agent_server::AgentServerResponse {
                    status: reply.status.unwrap_or(200),
                    headers: reply.headers.unwrap_or_default(),
                    body: reply.body.unwrap_or_default(),
                })
            }
            Ok(reply) => chain_core::agent_server::DispatchOutcome::HandlerFailed(
                reply.error.unwrap_or_else(|| "agent-server handler failed".to_string()),
            ),
            Err(_) => {
                // Clear the slot so a late reply that arrives after we've
                // already given up isn't mistaken for the *next*
                // request's answer.
                *pending.lock().expect("agent-server pending mutex poisoned") = None;
                chain_core::agent_server::DispatchOutcome::Timeout
            }
        }
    };

    let handle = chain_core::agent_server::start(port, dispatch).map_err(to_agent_server_command_error)?;
    let bound_port = handle.port();
    *handle_guard = Some(handle);
    Ok(bound_port)
}

#[tauri::command]
fn agent_server_stop(state: tauri::State<AgentServerState>) -> Result<(), String> {
    // Idempotent — stopping an already-stopped (or never-started) server
    // is a no-op success, same reasoning files_delete's idempotency uses.
    if let Some(handle) = state.handle.lock().expect("agent-server mutex poisoned").take() {
        handle.stop();
    }
    Ok(())
}

// Callback target for the JS handler the app registered via
// desktop.agentServer.start() — see packages/sdk/src/agent-server.ts.
#[tauri::command]
fn __chain_agent_server_respond(
    state: tauri::State<AgentServerState>,
    id: u64,
    ok: bool,
    status: Option<u16>,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
    error: Option<String>,
) {
    let mut guard = state.pending.lock().expect("agent-server pending mutex poisoned");
    match guard.take() {
        Some((pending_id, tx)) if pending_id == id => {
            let _ = tx.send(AgentServerReply { ok, status, headers, body, error });
        }
        // Stale reply for a request we already gave up waiting on (or a
        // mismatched id) — put the actually-pending one back untouched.
        other => *guard = other,
    }
}

// Bridges the process-runner capability contract (capabilities/process-runner
// in chain-sdk). chain_core::process_runner owns the actual spawn/pipe-
// reading and knows nothing about Tauri. Unlike agent-server, this
// direction needs no blocking wait for a JS reply — output/exit are
// purely outbound events the webview listens for; the only inbound call
// is `process_runner_kill`. See
// agent-docs/capabilities/process-runner/CONTRACT.md.
#[derive(Default)]
struct ProcessRunnerState {
    // Arc'd so the on_output/on_exit closures (which outlive the
    // `process_runner_run` command call that created them) can hold
    // their own clone, same reasoning AgentServerState's `pending` field
    // uses.
    processes: Arc<Mutex<HashMap<String, chain_core::process_runner::ProcessHandle>>>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessOutputPayload {
    id: String,
    stream: &'static str,
    data: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessExitPayload {
    id: String,
    code: Option<i32>,
    killed: bool,
}

fn to_process_runner_command_error(e: chain_core::process_runner::ProcessRunnerError) -> String {
    use chain_core::process_runner::ProcessRunnerError::*;
    match e {
        InvalidArgument(m) => format!("INVALID_ARGUMENT: {m}"),
        NotFound(m) => format!("NOT_FOUND: {m}"),
        PermissionDenied(m) => format!("PERMISSION_DENIED: {m}"),
        Other(m) => m,
    }
}

// `id` is caller-supplied (generated in packages/sdk/src/process-runner.ts),
// not native-generated — deliberately, so the JS side can register its
// event listener for this exact id *before* invoking this command at
// all. A fast process (spawn, print, exit) can finish well within one
// IPC round trip; if native generated the id and handed it back, output
// for a very fast process could already have been emitted (and lost, no
// listener yet) before that id ever reached JS. Caller-supplied ids
// close that race entirely — see packages/sdk/src/process-runner.ts's
// comments.
// One argv element: a plain string, or a `desktop.files` reference that's
// replaced by that managed file's path here, so JS never sees it — see
// process-runner/CONTRACT.md's "File-reference arguments".
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum ProcessArg {
    Text(String),
    File {
        #[serde(rename = "fileReference")]
        file_reference: String,
    },
}

#[tauri::command]
fn process_runner_run(
    app: tauri::AppHandle,
    state: tauri::State<ProcessRunnerState>,
    files_state: tauri::State<FilesState>,
    id: String,
    command: String,
    args: Vec<ProcessArg>,
    stdin: Option<String>,
) -> Result<(), String> {
    // Resolved before anything spawns: an unknown reference rejects run()
    // with NOT_FOUND and no process is ever started.
    let args = args
        .into_iter()
        .map(|arg| match arg {
            ProcessArg::Text(text) => Ok(text),
            ProcessArg::File { file_reference } => {
                with_files(&app, &files_state, |files| files.process_path(&file_reference))
            }
        })
        .collect::<Result<Vec<String>, String>>()?;

    let processes = Arc::clone(&state.processes);
    let processes_for_exit = Arc::clone(&processes);
    let app_for_output = app.clone();
    let id_for_output = id.clone();
    let id_for_exit = id.clone();

    let handle = chain_core::process_runner::run(
        &command,
        &args,
        stdin,
        move |chunk| {
            let stream = match chunk.stream {
                chain_core::process_runner::ProcessStream::Stdout => "stdout",
                chain_core::process_runner::ProcessStream::Stderr => "stderr",
            };
            let _ = app_for_output.emit(
                "chain://process-output",
                ProcessOutputPayload { id: id_for_output.clone(), stream, data: chunk.data },
            );
        },
        move |exit| {
            let _ = app.emit(
                "chain://process-exit",
                ProcessExitPayload { id: id_for_exit.clone(), code: exit.code, killed: exit.killed },
            );
            processes_for_exit.lock().expect("process-runner mutex poisoned").remove(&id_for_exit);
        },
    )
    .map_err(to_process_runner_command_error)?;

    processes.lock().expect("process-runner mutex poisoned").insert(id.clone(), handle);
    Ok(())
}

#[tauri::command]
fn process_runner_kill(state: tauri::State<ProcessRunnerState>, id: String) -> Result<(), String> {
    // Idempotent — a missing id (already exited and cleaned up, or never
    // existed) is treated as already-stopped, not an error, same
    // reasoning ProcessHandle::kill() itself already uses.
    if let Some(handle) = state.processes.lock().expect("process-runner mutex poisoned").get(&id) {
        handle.kill().map_err(to_process_runner_command_error)?;
    }
    Ok(())
}

// Callback target for the dev inspector's injected JS — see dev_inspector.rs.
// Always registered (so `generate_handler!` below stays unconditional), but
// only ever invoked when the `chain-dev-inspector` feature actually started
// the bridge — see that module's doc comment for why this command's own
// signature stays feature-independent.
#[tauri::command]
fn __chain_inspector_report(
    state: tauri::State<dev_inspector::InspectorState>,
    ok: bool,
    result: Option<String>,
    error: Option<String>,
) {
    dev_inspector::report(&state, ok, result, error);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(StorageState(Mutex::new(None)))
        .manage(FilesState(Mutex::new(None)))
        .manage(AgentServerState::default())
        .manage(ProcessRunnerState::default())
        .setup(|_app| {
            _app.manage(dev_inspector::InspectorState::default());
            #[cfg(feature = "chain-dev-inspector")]
            dev_inspector::start(_app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            get_platform_info,
            storage_migrate,
            storage_query,
            storage_execute,
            files_write,
            files_read,
            files_resolve_path,
            files_delete,
            files_pick,
            files_save,
            http_get,
            agent_server_start,
            agent_server_stop,
            __chain_agent_server_respond,
            process_runner_run,
            process_runner_kill,
            __chain_inspector_report
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
