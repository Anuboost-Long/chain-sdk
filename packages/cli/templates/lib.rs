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

fn to_open_command_error(e: chain_core::files::OpenError) -> String {
    use chain_core::files::OpenError::*;
    match e {
        NotFound(m) => format!("NOT_FOUND: {m}"),
        Unsupported(m) => format!("UNSUPPORTED: {m}"),
        Unavailable(m) => format!("UNAVAILABLE: {m}"),
        Other(m) => m,
    }
}

// Sync, so Tauri runs them on the main thread, where NSWorkspace and
// ShellExecute belong.
#[tauri::command]
fn files_open(app: tauri::AppHandle, state: tauri::State<FilesState>, reference: String) -> Result<(), String> {
    with_files(&app, &state, |files| Ok(files.open_in_app(&reference)))?.map_err(to_open_command_error)
}

#[tauri::command]
fn files_reveal(app: tauri::AppHandle, state: tauri::State<FilesState>, reference: String) -> Result<(), String> {
    with_files(&app, &state, |files| Ok(files.reveal(&reference)))?.map_err(to_open_command_error)
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
// Any method, headers, params and body — the SDK has already serialized
// params and encoded the body (packages/sdk/src/http.ts).
#[tauri::command]
async fn http_request(request: chain_core::http::HttpRequest) -> Result<chain_core::http::HttpResponse, String> {
    chain_core::http::request(request).await.map_err(to_http_command_error)
}

// responseType "bytes": one raw buffer (head JSON, then the body — see
// chain_core::http::request_bytes), never a JSON number array.
#[tauri::command]
async fn http_request_bytes(request: chain_core::http::HttpRequest) -> Result<tauri::ipc::Response, String> {
    chain_core::http::request_bytes(request).await.map(tauri::ipc::Response::new).map_err(to_http_command_error)
}

fn to_http_command_error(e: chain_core::http::HttpError) -> String {
    match e {
        chain_core::http::HttpError::InvalidUrl(m) => format!("INVALID_ARGUMENT: {m}"),
        chain_core::http::HttpError::Unavailable(m) => format!("UNAVAILABLE: {m}"),
        chain_core::http::HttpError::TooLarge(m) => format!("TOO_LARGE: {m}"),
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

// Bridges the models capability contract (capabilities/models in
// chain-sdk). Data-only model packs live in a `models/` sibling of
// `files/` in this app's data directory; engines get them by id and paths
// never reach JS. Progress goes out as `chain://models-progress` events
// keyed by model id (one install per id at a time).
#[derive(Default)]
struct ModelsState {
    models: Mutex<Option<chain_core::models::Models>>,
    installs: chain_core::models::Installs,
}

fn models_of(app: &tauri::AppHandle, state: &ModelsState) -> Result<chain_core::models::Models, String> {
    let mut guard = state.models.lock().expect("models mutex poisoned");
    if guard.is_none() {
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        *guard = Some(chain_core::models::Models::open(&dir.join("models")).map_err(to_models_command_error)?);
    }
    Ok(guard.clone().expect("just initialized above"))
}

// The SDK maps these prefixes onto ChainErrorCode — see packages/sdk/src/models.ts.
fn to_models_command_error(e: chain_core::models::ModelsError) -> String {
    use chain_core::models::ModelsError::*;
    match e {
        InvalidArgument(m) => format!("INVALID_ARGUMENT: {m}"),
        NotFound(m) => format!("NOT_FOUND: {m}"),
        Unavailable(m) => format!("UNAVAILABLE: {m}"),
        Integrity(m) => format!("INTEGRITY_FAILED: {m}"),
        Cancelled => "CANCELLED: the install was cancelled".to_string(),
        Other(m) => m,
    }
}

#[derive(Clone, serde::Serialize)]
struct ModelsProgressPayload {
    id: String,
    received: u64,
    total: Option<u64>,
}

#[tauri::command]
async fn models_install(
    app: tauri::AppHandle,
    state: tauri::State<'_, ModelsState>,
    manifest: chain_core::models::ModelManifest,
) -> Result<chain_core::models::InstalledModel, String> {
    let models = models_of(&app, &state)?;
    let ticket = state.installs.begin(&manifest.id).map_err(to_models_command_error)?;
    let id = manifest.id.clone();
    let download = models
        .download(&manifest, &ticket, |received, total| {
            let _ = app.emit("chain://models-progress", ModelsProgressPayload { id: id.clone(), received, total });
        })
        .await
        .map_err(to_models_command_error)?;
    tauri::async_runtime::spawn_blocking(move || models.install(download, &ticket))
        .await
        .map_err(|e| e.to_string())?
        .map_err(to_models_command_error)
}

#[tauri::command]
fn models_cancel(state: tauri::State<ModelsState>, id: String) {
    state.installs.cancel(&id);
}

#[tauri::command]
async fn models_list(
    app: tauri::AppHandle,
    state: tauri::State<'_, ModelsState>,
) -> Result<Vec<chain_core::models::InstalledModel>, String> {
    let models = models_of(&app, &state)?;
    tauri::async_runtime::spawn_blocking(move || models.list())
        .await
        .map_err(|e| e.to_string())?
        .map_err(to_models_command_error)
}

#[tauri::command]
async fn models_remove(
    app: tauri::AppHandle,
    state: tauri::State<'_, ModelsState>,
    id: String,
) -> Result<(), String> {
    // Removing a model mid-install cancels that install first.
    state.installs.cancel(&id);
    let models = models_of(&app, &state)?;
    tauri::async_runtime::spawn_blocking(move || models.remove(&id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(to_models_command_error)
}

// Bridges the tts capability contract (capabilities/tts in chain-sdk).
// Real only when chain-core is built with its `tts` feature — apps opt in
// with package.json "chain": { "gpl": true }, since it links espeak-ng
// (GPL-3.0). Otherwise both commands reject UNSUPPORTED.
fn to_tts_command_error(e: chain_core::sherpa::tts::TtsError) -> String {
    use chain_core::sherpa::tts::TtsError::*;
    match e {
        InvalidArgument(m) => format!("INVALID_ARGUMENT: {m}"),
        Unsupported(m) => format!("UNSUPPORTED: {m}"),
        Unavailable(m) => format!("UNAVAILABLE: {m}"),
        Cancelled => "CANCELLED: the compile was cancelled".to_string(),
        Other(m) => m,
    }
}

fn resolve_voice(
    models: &chain_core::models::Models,
    model_id: &str,
    mut config: chain_core::sherpa::tts::TtsModelConfig,
) -> Result<chain_core::sherpa::tts::TtsModelConfig, String> {
    for file in config.files_mut() {
        *file = models.resolve(model_id, file).map_err(to_models_command_error)?.to_string_lossy().into_owned();
    }
    for dir in config.dirs_mut() {
        *dir = models.resolve_dir(model_id, dir).map_err(to_models_command_error)?.to_string_lossy().into_owned();
    }
    Ok(config)
}

#[tauri::command]
async fn tts_voices(
    app: tauri::AppHandle,
    models_state: tauri::State<'_, ModelsState>,
    model_id: String,
    config: chain_core::sherpa::tts::TtsModelConfig,
) -> Result<Vec<chain_core::sherpa::tts::Voice>, String> {
    let config = resolve_voice(&models_of(&app, &models_state)?, &model_id, config)?;
    tauri::async_runtime::spawn_blocking(move || chain_core::sherpa::tts::voices(&config))
        .await
        .map_err(|e| e.to_string())?
        .map_err(to_tts_command_error)
}

// Writes the WAV through the files capability and returns its reference,
// so the app plays it with files.url() and deletes it like any other file.
#[tauri::command]
async fn tts_synthesize(
    app: tauri::AppHandle,
    files_state: tauri::State<'_, FilesState>,
    models_state: tauri::State<'_, ModelsState>,
    text: String,
    model_id: String,
    config: chain_core::sherpa::tts::TtsModelConfig,
    voice: Option<i32>,
    speed: Option<f32>,
) -> Result<String, String> {
    let config = resolve_voice(&models_of(&app, &models_state)?, &model_id, config)?;
    let wav = tauri::async_runtime::spawn_blocking(move || {
        chain_core::sherpa::tts::synthesize(&text, &config, voice.unwrap_or(0), speed.unwrap_or(1.0))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(to_tts_command_error)?;
    with_files(&app, &files_state, |files| files.write(&wav, Some("wav")))
}

#[derive(Clone, serde::Serialize)]
struct TtsProgressPayload {
    id: String,
    fraction: f64,
}

#[derive(serde::Serialize)]
struct CompiledAudio {
    file: String,
    duration: f64,
    segments: Vec<chain_core::sherpa::tts::SegmentTiming>,
}

// Encodes into a temp file and only moves it into files on success, so a
// cancelled or failed compile leaves nothing behind. Progress goes out as
// `chain://tts-progress` tagged with the caller's `id`, like speech's.
#[tauri::command]
async fn tts_compile(
    app: tauri::AppHandle,
    files_state: tauri::State<'_, FilesState>,
    models_state: tauri::State<'_, ModelsState>,
    id: String,
    segments: Vec<String>,
    model_id: String,
    config: chain_core::sherpa::tts::TtsModelConfig,
    voice: Option<i32>,
    speed: Option<f32>,
) -> Result<CompiledAudio, String> {
    let config = resolve_voice(&models_of(&app, &models_state)?, &model_id, config)?;
    // Our own name, never the caller's id: a call rejected UNAVAILABLE
    // must not remove the running compile's file.
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let temp = std::env::temp_dir().join(format!("chain-tts-{}-{nanos}.m4a", std::process::id()));
    let temp_for_job = temp.clone();
    let app_for_job = app.clone();
    let compiled = tauri::async_runtime::spawn_blocking(move || {
        chain_core::sherpa::tts::compile(&segments, &config, voice.unwrap_or(0), speed.unwrap_or(1.0), &temp_for_job, |fraction| {
            let _ = app_for_job.emit("chain://tts-progress", TtsProgressPayload { id: id.clone(), fraction });
        })
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|result| result.map_err(to_tts_command_error));
    let compiled = match compiled {
        Ok(compiled) => compiled,
        Err(e) => {
            let _ = std::fs::remove_file(&temp);
            return Err(e);
        }
    };
    let file = with_files(&app, &files_state, |files| files.adopt(&temp, Some("m4a")))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&temp);
        })?;
    Ok(CompiledAudio { file, duration: compiled.duration, segments: compiled.segments })
}

#[tauri::command]
fn tts_cancel() {
    chain_core::sherpa::tts::cancel_compile();
}

// Bridges the speech capability contract (capabilities/speech in
// chain-sdk). The audio is a `desktop.files` reference resolved to its
// path here — an hour of audio never crosses IPC. Progress goes out as
// `chain://speech-progress` events tagged with the caller-supplied `id`,
// same reasoning process_runner_run's ids use: the SDK listens before it
// invokes. chain_core::speech allows one transcription at a time.
#[derive(Clone, serde::Serialize)]
struct SpeechProgressPayload {
    id: String,
    fraction: f64,
}

fn to_speech_command_error(e: chain_core::speech::SpeechError) -> String {
    use chain_core::speech::SpeechError::*;
    match e {
        Unsupported(m) => format!("UNSUPPORTED: {m}"),
        Unavailable(m) => format!("UNAVAILABLE: {m}"),
        PermissionDenied(m) => format!("PERMISSION_DENIED: {m}"),
        Cancelled => "CANCELLED: the transcription was cancelled".to_string(),
        Other(m) => m,
    }
}

// `engine` picks an installed sherpa-onnx model instead of the OS
// recognizer; its file names are resolved inside the model here.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpeechEngine {
    model_id: String,
    config: chain_core::sherpa::AsrModelConfig,
    vad: SpeechVad,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpeechVad {
    model_id: String,
    model: String,
}

fn resolve_engine(models: &chain_core::models::Models, engine: SpeechEngine) -> Result<chain_core::sherpa::Engine, String> {
    let mut asr = engine.config;
    for file in asr.files_mut() {
        *file = models.resolve(&engine.model_id, file).map_err(to_models_command_error)?.to_string_lossy().into_owned();
    }
    let vad_model = models.resolve(&engine.vad.model_id, &engine.vad.model).map_err(to_models_command_error)?;
    Ok(chain_core::sherpa::Engine { asr, vad_model })
}

#[tauri::command]
async fn speech_transcribe(
    app: tauri::AppHandle,
    files_state: tauri::State<'_, FilesState>,
    models_state: tauri::State<'_, ModelsState>,
    id: String,
    reference: String,
    locale: Option<String>,
    engine: Option<SpeechEngine>,
) -> Result<chain_core::speech::Transcript, String> {
    let path = with_files(&app, &files_state, |files| files.process_path(&reference))?;
    let engine = match engine {
        Some(engine) => Some(resolve_engine(&models_of(&app, &models_state)?, engine)?),
        None => None,
    };
    tauri::async_runtime::spawn_blocking(move || {
        let path = std::path::Path::new(&path);
        let progress = |fraction| {
            let _ = app.emit("chain://speech-progress", SpeechProgressPayload { id: id.clone(), fraction });
        };
        match &engine {
            Some(engine) => chain_core::speech::transcribe_with_engine(path, engine, locale.as_deref(), progress),
            None => chain_core::speech::transcribe(path, locale.as_deref(), progress),
        }
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(to_speech_command_error)
}

#[tauri::command]
fn speech_cancel() {
    chain_core::speech::cancel();
}

#[tauri::command]
async fn speech_locales() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(chain_core::speech::locales)
        .await
        .map_err(|e| e.to_string())?
        .map_err(to_speech_command_error)
}

// Bridges the vision capability contract (capabilities/vision in
// chain-sdk). The image arrives as the raw IPC body — not a JSON number
// array, which would turn a 10 MB photo into ~40 MB of text — with the
// options JSON in a header. Recognition runs on a blocking thread so a
// large image never stalls Tauri's async runtime.
#[derive(serde::Deserialize, Default)]
struct VisionOptions {
    languages: Option<Vec<String>>,
    accurate: Option<bool>,
}

fn to_vision_command_error(e: chain_core::vision::VisionError) -> String {
    use chain_core::vision::VisionError::*;
    match e {
        InvalidImage(m) => format!("INVALID_ARGUMENT: {m}"),
        Unsupported(m) => format!("UNSUPPORTED: {m}"),
        Other(m) => m,
    }
}

#[tauri::command]
async fn vision_recognize_text(
    request: tauri::ipc::Request<'_>,
) -> Result<chain_core::vision::RecognizedText, String> {
    let (image, options) = vision_input(&request)?;
    let options = chain_core::vision::RecognizeOptions {
        languages: options.languages.unwrap_or_default(),
        accurate: options.accurate.unwrap_or(true),
    };
    tauri::async_runtime::spawn_blocking(move || chain_core::vision::recognize_text(&image, &options))
        .await
        .map_err(|e| e.to_string())?
        .map_err(to_vision_command_error)
}

#[tauri::command]
async fn vision_recognize_document(
    request: tauri::ipc::Request<'_>,
) -> Result<chain_core::vision::RecognizedDocument, String> {
    let (image, options) = vision_input(&request)?;
    let languages = options.languages.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || chain_core::vision::recognize_document(&image, &languages))
        .await
        .map_err(|e| e.to_string())?
        .map_err(to_vision_command_error)
}

fn vision_input(request: &tauri::ipc::Request<'_>) -> Result<(Vec<u8>, VisionOptions), String> {
    let tauri::ipc::InvokeBody::Raw(image) = request.body() else {
        return Err("INVALID_ARGUMENT: the image must be sent as raw bytes".to_string());
    };
    let options = match request.headers().get("chain-vision-options") {
        Some(header) => serde_json::from_slice(header.as_bytes()).map_err(|e| format!("INVALID_ARGUMENT: {e}"))?,
        None => VisionOptions::default(),
    };
    Ok((image.clone(), options))
}

#[tauri::command]
async fn vision_languages() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(chain_core::vision::languages)
        .await
        .map_err(|e| e.to_string())?
        .map_err(to_vision_command_error)
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
    // So macOS asks for the microphone etc. as this app, not as the
    // terminal that ran `chain dev` — see chain_core::dev_launch.
    #[cfg(feature = "chain-dev-inspector")]
    chain_core::dev_launch::become_responsible_for_itself();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(StorageState(Mutex::new(None)))
        .manage(FilesState(Mutex::new(None)))
        .manage(AgentServerState::default())
        .manage(ProcessRunnerState::default())
        .manage(ModelsState::default())
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
            files_open,
            files_reveal,
            http_request,
            http_request_bytes,
            agent_server_start,
            agent_server_stop,
            __chain_agent_server_respond,
            process_runner_run,
            process_runner_kill,
            vision_recognize_text,
            vision_recognize_document,
            vision_languages,
            speech_transcribe,
            speech_cancel,
            speech_locales,
            models_install,
            models_cancel,
            models_list,
            models_remove,
            tts_voices,
            tts_synthesize,
            tts_compile,
            tts_cancel,
            __chain_inspector_report
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
