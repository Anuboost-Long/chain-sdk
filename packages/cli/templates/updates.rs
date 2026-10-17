// Bridges the updates capability contract (capabilities/updates in
// chain-sdk): a built app replacing itself. tauri-plugin-updater reads the
// feed, downloads and verifies the Ed25519 signature; chain_core::updates
// decides whether this run can update and swaps the new app into place
// (atomically — the plugin's own install isn't). See
// agent-docs/capabilities/updates/.

use std::sync::Mutex;

use chain_core::updates::{self as core, Unsupported};
use tauri::{Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

const PACKAGE_JSON: &str = include_str!(env!("CHAIN_PACKAGE_JSON"));

#[derive(Default)]
pub struct UpdatesState {
    // What the last check() found.
    found: Mutex<Option<Update>>,
    // A verified update (version, archive) waiting for install or quit.
    downloaded: Mutex<Option<(String, Vec<u8>)>>,
}

fn config() -> Option<core::Config> {
    core::Config::from_package_json(PACKAGE_JSON).ok().flatten()
}

/// Gives tauri-plugin-updater its settings from package.json's
/// "chain.updates". A bad "chain.updates" stops the app at launch, like a
/// bad "chain.window".
pub fn configure<R: tauri::Runtime>(context: &mut tauri::Context<R>) -> Result<(), String> {
    let config = core::Config::from_package_json(PACKAGE_JSON).map_err(|e| format!("{e:?}"))?;
    let settings = match &config {
        Some(config) => serde_json::json!({
            "pubkey": config.public_key,
            "endpoints": [config.feed],
            "dangerousInsecureTransportProtocol": config.insecure_feed(),
            // The feed isn't signed: without this a tampered one could pass
            // an older genuine release off as newer.
            "requireSignedVersion": true
        }),
        None => serde_json::json!({ "pubkey": "", "endpoints": [] }),
    };
    context.config_mut().plugins.0.insert("updater".to_string(), settings);
    Ok(())
}

pub fn plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R, tauri_plugin_updater::Config> {
    tauri_plugin_updater::Builder::new().build()
}

fn replaceable() -> Result<std::path::PathBuf, Unsupported> {
    let exe = std::env::current_exe().map_err(|_| Unsupported::DevBuild)?;
    core::replaceable_bundle(&exe, config().is_some(), tauri::is_dev())
}

fn unsupported(reason: Unsupported) -> String {
    format!("UNSUPPORTED: {}", reason.message())
}

// The SDK maps these prefixes onto ChainErrorCode — see packages/sdk/src/updates.ts.
fn to_updates_command_error(e: tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error::*;
    match e {
        Minisign(_) | Base64(_) | SignatureUtf8(_) | SignedVersionMismatch { .. } => {
            format!("INTEGRITY_FAILED: the update's signature doesn't verify: {e}")
        }
        Reqwest(_) | Network(_) | ReleaseNotFound | Serialization(_) | Semver(_) | TargetNotFound(_) | TargetsNotFound(_) => {
            format!("UNAVAILABLE: {e}")
        }
        other => other.to_string(),
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<Unsupported>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'static str>,
    current_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    downloaded: Option<String>,
}

#[tauri::command]
pub fn updates_status(app: tauri::AppHandle, state: tauri::State<UpdatesState>) -> Status {
    let unsupported = replaceable().err();
    Status {
        supported: unsupported.is_none(),
        reason: unsupported,
        message: unsupported.map(Unsupported::message),
        current_version: app.package_info().version.to_string(),
        downloaded: state.downloaded.lock().expect("updates mutex poisoned").as_ref().map(|(version, _)| version.clone()),
    }
}

#[derive(serde::Serialize)]
pub struct Available {
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
}

#[tauri::command]
pub async fn updates_check(app: tauri::AppHandle) -> Result<Option<Available>, String> {
    replaceable().map_err(unsupported)?;
    let updater = app.updater().map_err(to_updates_command_error)?;
    let found = updater.check().await.map_err(to_updates_command_error)?;
    let available = found.as_ref().map(|update| Available {
        version: update.version.clone(),
        date: update.raw_json.get("pub_date").and_then(|date| date.as_str()).map(str::to_string),
        notes: update.body.clone(),
    });
    *app.state::<UpdatesState>().found.lock().expect("updates mutex poisoned") = found;
    Ok(available)
}

#[derive(Clone, serde::Serialize)]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

#[tauri::command]
pub async fn updates_download(app: tauri::AppHandle) -> Result<(), String> {
    replaceable().map_err(unsupported)?;
    let update = app.state::<UpdatesState>().found.lock().expect("updates mutex poisoned").clone();
    let update = update.ok_or_else(|| "NOT_FOUND: no update to download — check() first".to_string())?;
    let mut downloaded = 0u64;
    let progress = app.clone();
    let bytes = update
        .download(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.emit("chain://updates-progress", Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(to_updates_command_error)?;
    *app.state::<UpdatesState>().downloaded.lock().expect("updates mutex poisoned") = Some((update.version.clone(), bytes));
    Ok(())
}

fn install(app: &tauri::AppHandle) -> Result<bool, String> {
    let bundle = replaceable().map_err(unsupported)?;
    let Some((_, archive)) = app.state::<UpdatesState>().downloaded.lock().expect("updates mutex poisoned").take() else {
        return Ok(false);
    };
    core::install(&bundle, &archive).map_err(|e| match e {
        core::UpdatesError::Install(m) if m.contains("No space left") => format!("UNAVAILABLE: not enough disk space: {m}"),
        core::UpdatesError::Install(m) | core::UpdatesError::InvalidConfig(m) => m,
    })?;
    Ok(true)
}

#[tauri::command]
pub async fn updates_install(app: tauri::AppHandle) -> Result<(), String> {
    if !install(&app)? {
        return Err("NOT_FOUND: no update is downloaded — download() first".to_string());
    }
    app.restart();
}

/// A downloaded update installs when the app quits normally.
pub fn on_exit(app: &tauri::AppHandle) {
    if let Err(e) = install(app) {
        eprintln!("[chain] the downloaded update couldn't be installed on quit: {e}");
    }
}
