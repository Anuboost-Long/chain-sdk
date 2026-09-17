//! Dev-only automation bridge for `chain inspect` — lets an external
//! driver eval JS in the running app's webview during `chain dev`, since
//! Tauri's native webviews (WKWebView/WebView2/WebKitGTK) have no Chrome
//! DevTools Protocol the way Electron's bundled Chromium does.
//!
//! `InspectorState`/`report()` stay compiled unconditionally so the
//! `__chain_inspector_report` command in lib.rs has a stable signature
//! either way (`#[cfg]` on an individual command *parameter* doesn't
//! reliably survive Tauri's command macro — cfg per top-level item here
//! instead). Everything that actually listens on a socket or touches the
//! webview is gated behind the `chain-dev-inspector` Cargo feature, which
//! only `chain dev` ever enables (never `chain build` — see
//! packages/cli/src/dev.ts), so a release binary never runs any of it.
//! Not a Chain SDK capability: nothing an app ever calls, only the
//! developer/agent via `chain inspect`. See
//! agent-docs/framework/command/README.md for the wire protocol —
//! `eval` and `rect` are the only two wire commands; `chain inspect`'s
//! `screenshot`/`drag` are Node-side (screencapture/cliclick), built on
//! `rect`, deliberately not implemented here (see that README section for
//! why). Non-goals: no auto-reconnect; token is a same-machine speed
//! bump, not cryptographic auth.

use std::sync::mpsc;
use std::sync::Mutex;

#[cfg(feature = "chain-dev-inspector")]
use std::io::{BufRead, BufReader, Write};
#[cfg(feature = "chain-dev-inspector")]
use std::net::{TcpListener, TcpStream};
#[cfg(feature = "chain-dev-inspector")]
use std::time::{Duration, SystemTime, UNIX_EPOCH};
#[cfg(feature = "chain-dev-inspector")]
use tauri::Manager;

#[derive(Default)]
pub struct InspectorState(Mutex<Option<mpsc::Sender<InspectorReply>>>);

// Fields are only read by run_eval/handle_connection, which don't exist
// without the feature — allow dead_code rather than cfg-gating the fields
// themselves (report()'s signature must stay stable either way).
#[cfg_attr(not(feature = "chain-dev-inspector"), allow(dead_code))]
pub struct InspectorReply {
    ok: bool,
    result: Option<String>,
    error: Option<String>,
}

/// Called from the `__chain_inspector_report` command — the injected JS's
/// callback after it evaluates the requested code.
pub fn report(state: &InspectorState, ok: bool, result: Option<String>, error: Option<String>) {
    if let Some(tx) = state.0.lock().expect("inspector mutex poisoned").take() {
        let _ = tx.send(InspectorReply { ok, result, error });
    }
}

#[cfg(feature = "chain-dev-inspector")]
#[derive(serde::Deserialize)]
struct InspectRequest {
    id: u64,
    token: String,
    cmd: String,
    code: Option<String>,
}

/// A same-machine speed bump, not cryptographic auth (see module docs) —
/// just enough that another local process can't blind-guess it.
#[cfg(feature = "chain-dev-inspector")]
fn generate_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    let mut x = nanos ^ ((std::process::id() as u64) << 32) ^ 0x9E3779B97F4A7C15;
    let mut token = String::with_capacity(32);
    for _ in 0..32 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        token.push(std::char::from_digit((x % 16) as u32, 16).unwrap());
    }
    token
}

/// `target/` sits next to whichever binary is currently running (walking
/// up from `target/debug/<bin>`), so the info file always lands inside the
/// native project's own already-gitignored `target/` regardless of the
/// process's working directory.
#[cfg(feature = "chain-dev-inspector")]
fn info_file_path() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("chain-inspector.json")
}

#[cfg(feature = "chain-dev-inspector")]
pub fn start(app: tauri::AppHandle) {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(l) => l,
        Err(e) => {
            eprintln!("chain dev-inspector: failed to bind: {e}");
            return;
        }
    };
    let port = match listener.local_addr() {
        Ok(addr) => addr.port(),
        Err(_) => return,
    };
    let token = generate_token();

    let info = serde_json::json!({ "port": port, "token": token, "pid": std::process::id() }).to_string();
    if let Err(e) = std::fs::write(info_file_path(), info) {
        eprintln!("chain dev-inspector: failed to write info file: {e}");
    }
    eprintln!("chain dev-inspector: listening on 127.0.0.1:{port}");

    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            handle_connection(stream, app.clone(), token.clone());
        }
    });
}

/// One driver connects at a time (`chain inspect` is a REPL) — handled
/// serially in the accept loop, no per-connection thread needed.
#[cfg(feature = "chain-dev-inspector")]
fn handle_connection(mut stream: TcpStream, app: tauri::AppHandle, token: String) {
    let Ok(clone) = stream.try_clone() else { return };
    let reader = BufReader::new(clone);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let req: InspectRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let _ = writeln!(stream, "{}", serde_json::json!({ "ok": false, "error": format!("bad request: {e}") }));
                continue;
            }
        };
        if req.token != token {
            let _ = writeln!(stream, "{}", serde_json::json!({ "id": req.id, "ok": false, "error": "unauthorized" }));
            continue;
        }
        let reply = match req.cmd.as_str() {
            "eval" => run_eval(&app, &req.code.unwrap_or_default()),
            "rect" => run_rect(&app),
            other => InspectorReply { ok: false, result: None, error: Some(format!("unknown cmd: {other}")) },
        };
        let _ = writeln!(
            stream,
            "{}",
            serde_json::json!({ "id": req.id, "ok": reply.ok, "result": reply.result, "error": reply.error })
        );
    }
}

/// Tauri's `eval()` is fire-and-forget, so the requested code is wrapped to
/// report its own result back through `__chain_inspector_report` — this
/// blocks until that callback arrives (or times out).
#[cfg(feature = "chain-dev-inspector")]
fn run_eval(app: &tauri::AppHandle, code: &str) -> InspectorReply {
    let Some(window) = app.get_webview_window("main") else {
        return InspectorReply { ok: false, result: None, error: Some("no main window".into()) };
    };
    let (tx, rx) = mpsc::channel();
    *app.state::<InspectorState>().0.lock().expect("inspector mutex poisoned") = Some(tx);

    let wrapped = format!(
        "(function(){{ try {{ const __r = ({code}); \
         window.__TAURI_INTERNALS__.invoke('__chain_inspector_report', {{ ok: true, result: JSON.stringify(__r === undefined ? null : __r) }}); \
         }} catch (e) {{ \
         window.__TAURI_INTERNALS__.invoke('__chain_inspector_report', {{ ok: false, error: String((e && e.message) || e) }}); \
         }} }})()"
    );
    if let Err(e) = window.eval(&wrapped) {
        return InspectorReply { ok: false, result: None, error: Some(format!("eval failed: {e}")) };
    }
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(reply) => reply,
        Err(_) => InspectorReply { ok: false, result: None, error: Some("timed out waiting for result".into()) },
    }
}

/// Physical-pixel window/webview geometry — `chain inspect screenshot`/
/// `drag` use this to convert a DOM element's `getBoundingClientRect()`
/// (CSS pixels) into absolute on-screen coordinates for OS-level tools
/// (`screencapture`, `cliclick`) that know nothing about the webview.
#[cfg(feature = "chain-dev-inspector")]
fn run_rect(app: &tauri::AppHandle) -> InspectorReply {
    let Some(window) = app.get_webview_window("main") else {
        return InspectorReply { ok: false, result: None, error: Some("no main window".into()) };
    };
    let position = match window.inner_position() {
        Ok(p) => p,
        Err(e) => return InspectorReply { ok: false, result: None, error: Some(format!("inner_position failed: {e}")) },
    };
    let size = match window.inner_size() {
        Ok(s) => s,
        Err(e) => return InspectorReply { ok: false, result: None, error: Some(format!("inner_size failed: {e}")) },
    };
    let scale_factor = window.scale_factor().unwrap_or(1.0);
    let result = serde_json::json!({
        "x": position.x,
        "y": position.y,
        "width": size.width,
        "height": size.height,
        "scaleFactor": scale_factor
    })
    .to_string();
    InspectorReply { ok: true, result: Some(result), error: None }
}
