// Bridges the folders capability contract (capabilities/folders in
// chain-sdk): real paths inside folders the user picked or dropped, plus
// the read-only folders package.json declares under "chain.readOnlyFolders"
// (`chain dev`/`chain build` pass them in as CHAIN_READ_ONLY_FOLDERS). The
// grant checks and file work are chain_core::folders; this file only
// reaches the webview. See agent-docs/capabilities/folders/.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use chain_core::folders::{self as core, AppFolder, Change, Entry, FolderWatch, Folders, FoldersError, Grant, GrantSource};
use tauri::{Emitter, Manager, Runtime};

#[derive(Default)]
pub struct FoldersState {
    folders: OnceLock<Arc<Folders>>,
    // Keyed by the watch id the SDK generated, with the webview that owns it.
    watches: Mutex<HashMap<String, (String, FolderWatch)>>,
    // Webviews with at least one onDrop() subscriber.
    accepting_drops: Mutex<HashSet<String>>,
}

// The SDK maps these prefixes onto ChainErrorCode — see packages/sdk/src/folders.ts.
pub fn to_folders_command_error(e: FoldersError) -> String {
    match e {
        FoldersError::InvalidArgument(m) => format!("INVALID_ARGUMENT: {m}"),
        FoldersError::NotGranted(m) => format!("NOT_GRANTED: {m}"),
        FoldersError::PermissionDenied(m) => format!("PERMISSION_DENIED: {m}"),
        FoldersError::NotFound(m) => format!("NOT_FOUND: {m}"),
        FoldersError::Unavailable(m) => format!("UNAVAILABLE: {m}"),
        FoldersError::TooLarge(m) => format!("TOO_LARGE: {m}"),
        FoldersError::Unsupported(m) => format!("UNSUPPORTED: {m}"),
        FoldersError::Other(m) => m,
    }
}

pub fn folders<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<Arc<Folders>, String> {
    let state = app.state::<FoldersState>();
    if let Some(folders) = state.folders.get() {
        return Ok(Arc::clone(folders));
    }
    let declared: Vec<String> = match option_env!("CHAIN_READ_ONLY_FOLDERS") {
        Some(json) if !json.is_empty() => serde_json::from_str(json).map_err(|e| format!("CHAIN_READ_ONLY_FOLDERS: {e}"))?,
        _ => Vec::new(),
    };
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let temp_dir = app.path().temp_dir().map_err(|e| e.to_string())?.join(&app.config().identifier);
    let home = app.path().home_dir().ok();
    // The app's own folder is `app/` beside the grants file, never the data
    // folder itself, so the app can't write its own grants.
    let opened = Folders::open(&data_dir.join("folder-grants.json"), &declared, home.as_deref())
        .and_then(|folders| folders.with_app_folders(&data_dir.join("app"), &temp_dir))
        .map_err(to_folders_command_error)?;
    Ok(Arc::clone(state.folders.get_or_init(|| Arc::new(opened))))
}

// File work runs off the main thread, which is where sync commands run:
// a recursive listing of a monorepo shouldn't freeze the window.
async fn off_main<T: Send + 'static>(
    app: &tauri::AppHandle,
    work: impl FnOnce(&Folders) -> Result<T, FoldersError> + Send + 'static,
) -> Result<T, String> {
    let folders = folders(app)?;
    tauri::async_runtime::spawn_blocking(move || work(&folders))
        .await
        .map_err(|e| e.to_string())?
        .map_err(to_folders_command_error)
}

#[tauri::command]
pub async fn folders_pick(
    app: tauri::AppHandle,
    window: tauri::Window,
    multiple: Option<bool>,
    files: Option<bool>,
) -> Result<Vec<Grant>, String> {
    let options = core::PickOptions { multiple: multiple.unwrap_or(false), files: files.unwrap_or(false) };
    let chosen = core::pick(&window, &options).await.map_err(to_folders_command_error)?;
    off_main(&app, move |folders| chosen.iter().map(|path| folders.grant(path, GrantSource::Picked)).collect()).await
}

#[tauri::command]
pub fn folders_grants(app: tauri::AppHandle) -> Result<Vec<Grant>, String> {
    Ok(folders(&app)?.grants())
}

#[tauri::command]
pub fn folders_app_folder(app: tauri::AppHandle, folder: AppFolder) -> Result<Grant, String> {
    folders(&app)?.app_folder(folder).map_err(to_folders_command_error)
}

#[tauri::command]
pub fn folders_revoke(app: tauri::AppHandle, id: String) -> Result<(), String> {
    folders(&app)?.revoke(&id).map_err(to_folders_command_error)
}

#[tauri::command]
pub fn folders_accept_drops(webview: tauri::Webview, state: tauri::State<FoldersState>, accept: bool) {
    let mut accepting = state.accepting_drops.lock().expect("folders mutex poisoned");
    if accept {
        accepting.insert(webview.label().to_string());
    } else {
        accepting.remove(webview.label());
    }
}

#[tauri::command]
pub async fn folders_list(
    app: tauri::AppHandle,
    path: String,
    recursive: Option<bool>,
    skip_folders: Option<Vec<String>>,
) -> Result<Vec<Entry>, String> {
    let skip = skip_folders.unwrap_or_default();
    off_main(&app, move |f| f.list(&path, recursive.unwrap_or(false), &skip)).await
}

#[tauri::command]
pub async fn folders_stat(app: tauri::AppHandle, path: String) -> Result<Entry, String> {
    off_main(&app, move |f| f.stat(&path)).await
}

#[tauri::command]
pub async fn folders_exists(app: tauri::AppHandle, path: String) -> Result<bool, String> {
    off_main(&app, move |f| f.exists(&path)).await
}

#[tauri::command]
pub async fn folders_read_text(app: tauri::AppHandle, path: String, max_bytes: Option<u64>) -> Result<String, String> {
    off_main(&app, move |f| f.read_text(&path, max_bytes)).await
}

// Raw bytes, not a JSON number array (which would quadruple a 16 MB preview).
#[tauri::command]
pub async fn folders_read_bytes(
    app: tauri::AppHandle,
    path: String,
    offset: Option<u64>,
    length: Option<u64>,
    max_bytes: Option<u64>,
) -> Result<tauri::ipc::Response, String> {
    let bytes = off_main(&app, move |f| f.read_bytes(&path, offset.unwrap_or(0), length, max_bytes)).await?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub async fn folders_write_text(app: tauri::AppHandle, path: String, text: String) -> Result<(), String> {
    off_main(&app, move |f| f.write(&path, text.as_bytes())).await
}

// The raw IPC body is a u32 little-endian path length, the UTF-8 path,
// then the file's bytes — see packages/sdk/src/folders.ts.
#[tauri::command]
pub async fn folders_write_bytes(app: tauri::AppHandle, request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let tauri::ipc::InvokeBody::Raw(body) = request.body() else {
        return Err("INVALID_ARGUMENT: the bytes must be sent as a raw body".to_string());
    };
    let malformed = || "INVALID_ARGUMENT: malformed writeBytes body".to_string();
    let length = u32::from_le_bytes(body.get(..4).ok_or_else(malformed)?.try_into().map_err(|_| malformed())?) as usize;
    let path = std::str::from_utf8(body.get(4..4 + length).ok_or_else(malformed)?).map_err(|_| malformed())?.to_string();
    let bytes = body[4 + length..].to_vec();
    off_main(&app, move |f| f.write(&path, &bytes)).await
}

#[tauri::command]
pub async fn folders_create_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    off_main(&app, move |f| f.create_folder(&path)).await
}

#[tauri::command]
pub async fn folders_move(app: tauri::AppHandle, from: String, to: String) -> Result<(), String> {
    off_main(&app, move |f| f.move_entry(&from, &to)).await
}

#[tauri::command]
pub async fn folders_delete(app: tauri::AppHandle, path: String, to_trash: Option<bool>) -> Result<(), String> {
    off_main(&app, move |f| f.delete(&path, to_trash.unwrap_or(false))).await
}

#[derive(Clone, serde::Serialize)]
struct ChangesPayload {
    id: String,
    changes: Vec<Change>,
}

// `id` comes from the SDK, which registers its handler under it before
// invoking, so no early event can arrive for an id it doesn't know yet.
#[tauri::command]
pub async fn folders_watch(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    id: String,
    path: String,
    recursive: Option<bool>,
) -> Result<(), String> {
    let emitter = app.clone();
    let event_id = id.clone();
    let watch = off_main(&app, move |f| {
        f.watch(&path, recursive.unwrap_or(false), move |changes| {
            let _ = emitter.emit("chain://folders-change", ChangesPayload { id: event_id.clone(), changes });
        })
    })
    .await?;
    let state = app.state::<FoldersState>();
    state.watches.lock().expect("folders mutex poisoned").insert(id, (webview.label().to_string(), watch));
    Ok(())
}

#[tauri::command]
pub fn folders_unwatch(state: tauri::State<FoldersState>, id: String) {
    state.watches.lock().expect("folders mutex poisoned").remove(&id);
}

/// A reloaded page can't stop its old watches or receive its old drops.
pub fn release_page(webview: &tauri::Webview) {
    let state = webview.state::<FoldersState>();
    state.watches.lock().expect("folders mutex poisoned").retain(|_, (owner, _)| owner != webview.label());
    state.accepting_drops.lock().expect("folders mutex poisoned").remove(webview.label());
}

/// Grants what's dropped onto a window whose page subscribed with onDrop().
pub fn on_window_event<R: Runtime>(window: &tauri::Window<R>, event: &tauri::WindowEvent) {
    let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event else { return };
    let state = window.state::<FoldersState>();
    if !state.accepting_drops.lock().expect("folders mutex poisoned").contains(window.label()) {
        return;
    }
    let Ok(folders) = folders(window.app_handle()) else { return };
    let grants: Vec<Grant> = paths
        .iter()
        .filter_map(|path| match folders.grant(path, GrantSource::Dropped) {
            Ok(grant) => Some(grant),
            Err(e) => {
                eprintln!("[chain] couldn't grant dropped {}: {}", path.display(), e.message());
                None
            }
        })
        .collect();
    if !grants.is_empty() {
        let _ = window.emit_to(tauri::EventTarget::webview(window.label()), "chain://folders-dropped", grants);
    }
}
