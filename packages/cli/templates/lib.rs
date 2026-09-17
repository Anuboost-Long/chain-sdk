use std::sync::Mutex;
use tauri::Manager;

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
            __chain_inspector_report
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
