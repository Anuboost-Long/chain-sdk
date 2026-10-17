// Bridges the pdf capability contract (capabilities/pdf in chain-sdk): an
// HTML document rendered off-screen into a paginated PDF. The document
// renders in a hidden Tauri webview, so the app's asset-protocol URLs, CSS
// and fonts load there as they do in the app; options and the render
// itself are chain_core::pdf. See agent-docs/capabilities/pdf/.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use chain_core::pdf::{self as core, PdfError, RenderContext, RenderOptions};
use tauri::{Manager, Url, WebviewUrl, WebviewWindowBuilder};

// Longer than the longest timeout render() accepts, plus printing.
const RENDER_LIMIT: Duration = Duration::from_secs(15 * 60);

#[derive(Default)]
pub struct PdfState {
    // One at a time: each print runs modally on the main thread.
    rendering: tauri::async_runtime::Mutex<()>,
}

static NEXT_RENDER: AtomicU32 = AtomicU32::new(0);

// The SDK maps these prefixes onto ChainErrorCode — see packages/sdk/src/pdf.ts.
fn to_pdf_command_error(e: PdfError) -> String {
    match e {
        PdfError::InvalidArgument(m) => format!("INVALID_ARGUMENT: {m}"),
        PdfError::TimedOut(m) => format!("TIMEOUT: {m}"),
        PdfError::ResourceFailed(m) => format!("NOT_FOUND: {m}"),
        PdfError::Unsupported(m) => format!("UNSUPPORTED: {m}"),
        PdfError::Other(m) => m,
    }
}

// Sync, so it runs on the main thread, where AppKit's print settings belong.
#[tauri::command]
pub fn pdf_availability() -> core::Availability {
    core::availability()
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedPdf {
    reference: String,
    page_count: u32,
    size: u64,
}

#[tauri::command]
pub(crate) async fn pdf_render(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: tauri::State<'_, PdfState>,
    files_state: tauri::State<'_, crate::FilesState>,
    html: String,
    options: Option<serde_json::Value>,
) -> Result<RenderedPdf, String> {
    let options = RenderOptions::parse(options).map_err(to_pdf_command_error)?;
    let _rendering = state.rendering.lock().await;

    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("chain-pdf");
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't prepare the PDF: {e}"))?;
    let label = format!("{}{}", core::LABEL_PREFIX, NEXT_RENDER.fetch_add(1, Ordering::Relaxed));
    let out = dir.join(format!("{label}.pdf"));

    let mut builder = WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(Url::parse("about:blank").expect("valid URL")))
        .visible(false)
        .focused(false)
        .inner_size(800.0, 1000.0);
    if !options.run_scripts() {
        builder = builder.disable_javascript();
    }
    let window = builder.build().map_err(|e| format!("couldn't create the webview to render in: {e}"))?;

    let context = (
        out.to_string_lossy().into_owned(),
        webview.url().ok().map(String::from),
        app.package_info().name.clone(),
    );
    let (tx, rx) = mpsc::channel();
    let started = window.with_webview(move |render_webview| {
        let (out_path, page_url, app_name) = context;
        let request = options.request(
            html,
            RenderContext { out_path: &out_path, default_paper: core::default_paper(), page_url, app_name: &app_name },
        );
        let done: core::RenderDone = Box::new(move |result| {
            let _ = tx.send(result);
        });
        match request {
            #[cfg(target_os = "macos")]
            Ok(request) => core::render(render_webview.inner(), &request, done),
            #[cfg(not(target_os = "macos"))]
            Ok(request) => {
                let _ = render_webview;
                core::render(std::ptr::null_mut(), &request, done)
            }
            Err(e) => done(Err(e)),
        }
    });
    let printed = match started {
        Ok(()) => tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(RENDER_LIMIT))
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|_| Err(PdfError::Other("the PDF render never finished".to_string()))),
        Err(e) => Err(PdfError::Other(format!("couldn't reach the webview to render in: {e}"))),
    };
    let _ = window.destroy();

    let printed = match printed {
        Ok(printed) => printed,
        Err(e) => {
            let _ = std::fs::remove_file(&out);
            return Err(to_pdf_command_error(e));
        }
    };
    let reference = crate::with_files(&app, &files_state, |files| files.adopt(&out, Some("pdf")))?;
    Ok(RenderedPdf { reference, page_count: printed.page_count, size: printed.size })
}
