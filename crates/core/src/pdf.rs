//! PDF capability — see /agent-docs/capabilities/pdf/CONTRACT.md.
//! Options are parsed and checked here; the render itself happens in a
//! hidden webview the Tauri layer creates (templates/pdf.rs), driven by
//! swift/ChainPdf.swift on macOS.

use std::ffi::c_void;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The hidden render webviews' window labels start with this, so the app
/// shell can leave them out of what it does for its own windows.
pub const LABEL_PREFIX: &str = "chain-pdf-";

const POINTS_PER_MM: f64 = 72.0 / 25.4;
const A4: PaperDimensions = PaperDimensions { width: 210.0, height: 297.0 };
const LETTER: PaperDimensions = PaperDimensions { width: 215.9, height: 279.4 };
const DEFAULT_MARGIN_MM: f64 = 15.0;
const DEFAULT_TIMEOUT_MS: f64 = 15_000.0;
const MAX_TIMEOUT_MS: f64 = 600_000.0;

#[derive(Debug)]
pub enum PdfError {
    InvalidArgument(String),
    /// The document's images, stylesheets or fonts didn't load in time.
    TimedOut(String),
    /// One of them failed to load.
    ResourceFailed(String),
    Unsupported(String),
    Other(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaperDimensions {
    pub width: f64,
    pub height: f64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Paper {
    Named(String),
    Custom(PaperDimensions),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Margins {
    All(f64),
    Sides(MarginSides),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MarginSides {
    top: Option<f64>,
    right: Option<f64>,
    bottom: Option<f64>,
    left: Option<f64>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MarginText {
    #[serde(skip_serializing_if = "Option::is_none")]
    left: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    center: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    right: Option<String>,
}

/// `RenderPdfOptions` from contract.ts.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderOptions {
    paper: Option<Paper>,
    orientation: Option<String>,
    margins: Option<Margins>,
    print_background: Option<bool>,
    color_scheme: Option<String>,
    header: Option<MarginText>,
    footer: Option<MarginText>,
    title: Option<String>,
    author: Option<String>,
    creator: Option<String>,
    base_url: Option<String>,
    run_scripts: Option<bool>,
    timeout: Option<f64>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct MarginsInPoints {
    top: f64,
    right: f64,
    bottom: f64,
    left: f64,
}

/// What swift/ChainPdf.swift renders: lengths in points, the paper
/// already turned for its orientation.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    html: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
    out_path: String,
    width: f64,
    height: f64,
    landscape: bool,
    margins: MarginsInPoints,
    print_background: bool,
    dark: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    header: Option<MarginText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    footer: Option<MarginText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    creator: String,
    timeout_ms: f64,
}

/// What every render needs besides the app's own options.
pub struct RenderContext<'a> {
    pub out_path: &'a str,
    pub default_paper: PaperDimensions,
    /// The calling page's URL, used when the options give no `baseUrl`.
    pub page_url: Option<String>,
    pub app_name: &'a str,
}

fn invalid(message: impl Into<String>) -> PdfError {
    PdfError::InvalidArgument(message.into())
}

impl RenderOptions {
    /// `options` as the SDK sent it; null or absent is all defaults.
    pub fn parse(options: Option<Value>) -> Result<Self, PdfError> {
        match options {
            None | Some(Value::Null) => Ok(Self::default()),
            Some(value) => serde_json::from_value(value).map_err(|e| invalid(format!("invalid PDF options: {e}"))),
        }
    }

    pub fn run_scripts(&self) -> bool {
        self.run_scripts.unwrap_or(false)
    }

    pub fn request(self, html: String, context: RenderContext) -> Result<Request, PdfError> {
        let paper = match &self.paper {
            None => context.default_paper,
            Some(Paper::Named(name)) => match name.as_str() {
                "a4" => A4,
                "letter" => LETTER,
                other => return Err(invalid(format!("unknown paper {other:?}: use \"a4\", \"letter\" or {{ width, height }} in millimetres"))),
            },
            Some(Paper::Custom(PaperDimensions { width, height })) => {
                if ![width, height].iter().all(|v| v.is_finite() && **v > 0.0) {
                    return Err(invalid("paper width and height must be positive numbers of millimetres"));
                }
                PaperDimensions { width: width.min(*height), height: width.max(*height) }
            }
        };
        let landscape = match self.orientation.as_deref() {
            None | Some("portrait") => false,
            Some("landscape") => true,
            Some(other) => return Err(invalid(format!("unknown orientation {other:?}: use \"portrait\" or \"landscape\""))),
        };
        let (width, height) = if landscape { (paper.height, paper.width) } else { (paper.width, paper.height) };

        let margin = |side: Option<f64>| side.unwrap_or(DEFAULT_MARGIN_MM);
        let [top, right, bottom, left] = match self.margins {
            None => [DEFAULT_MARGIN_MM; 4],
            Some(Margins::All(all)) => [all; 4],
            Some(Margins::Sides(MarginSides { top, right, bottom, left })) => [margin(top), margin(right), margin(bottom), margin(left)],
        };
        if ![top, right, bottom, left].iter().all(|v| v.is_finite() && *v >= 0.0) {
            return Err(invalid("margins must be zero or more millimetres"));
        }
        if left + right >= width || top + bottom >= height {
            return Err(invalid(format!("the margins leave no room on {width} × {height} mm paper")));
        }

        let dark = match self.color_scheme.as_deref() {
            None | Some("light") => false,
            Some("dark") => true,
            Some(other) => return Err(invalid(format!("unknown colorScheme {other:?}: use \"light\" or \"dark\""))),
        };
        let timeout_ms = self.timeout.unwrap_or(DEFAULT_TIMEOUT_MS);
        if !(timeout_ms.is_finite() && timeout_ms > 0.0 && timeout_ms <= MAX_TIMEOUT_MS) {
            return Err(invalid(format!("timeout must be more than 0 and at most {MAX_TIMEOUT_MS} milliseconds")));
        }

        if let Some(base) = self.base_url.as_deref().filter(|base| !has_scheme(base)) {
            return Err(invalid(format!("baseUrl {base:?} isn't an absolute URL")));
        }

        let points = |mm: f64| mm * POINTS_PER_MM;
        Ok(Request {
            html,
            base_url: self.base_url.or(context.page_url),
            out_path: context.out_path.to_string(),
            width: points(width),
            height: points(height),
            landscape,
            margins: MarginsInPoints { top: points(top), right: points(right), bottom: points(bottom), left: points(left) },
            print_background: self.print_background.unwrap_or(true),
            dark,
            header: self.header,
            footer: self.footer,
            title: self.title,
            author: self.author,
            creator: self.creator.unwrap_or_else(|| context.app_name.to_string()),
            timeout_ms,
        })
    }
}

/// AppKit gives paper sizes in whole points, so A4 reads back as
/// 209.9 × 297.0 mm; within a millimetre it's the real thing.
#[cfg(any(target_os = "macos", test))]
fn known_paper(paper: PaperDimensions) -> PaperDimensions {
    let near = |known: PaperDimensions| (known.width - paper.width).abs() < 1.0 && (known.height - paper.height).abs() < 1.0;
    [A4, LETTER].into_iter().find(|&known| near(known)).unwrap_or(paper)
}

/// Starts with a URL scheme, like `tauri:`, `asset:` or `http:`.
fn has_scheme(url: &str) -> bool {
    url.split_once(':').is_some_and(|(scheme, _)| {
        scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    })
}

/// `PdfAvailability` from contract.ts.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    available: bool,
    default_paper: PaperDimensions,
    page_setup: bool,
    css_page_breaks: bool,
    css_page_size: bool,
    css_page_margin_boxes: bool,
    header: bool,
    footer: bool,
    print_background: bool,
    color_scheme: bool,
    metadata: bool,
    run_scripts: bool,
    selectable_text: bool,
    complex_script_search: bool,
    links: bool,
}

pub fn availability() -> Availability {
    let available = cfg!(target_os = "macos");
    Availability {
        available,
        default_paper: default_paper(),
        page_setup: available,
        css_page_breaks: available,
        // WebKit's print path takes the size from the print settings only,
        // has no margin boxes, and Quartz drops what joined letters stand
        // for — see research/MACOS.md.
        css_page_size: false,
        css_page_margin_boxes: false,
        header: available,
        footer: available,
        print_background: available,
        color_scheme: available,
        metadata: available,
        run_scripts: available,
        selectable_text: available,
        complex_script_search: false,
        links: available,
    }
}

/// What the render produced, at `RenderContext::out_path`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Printed {
    pub page_count: u32,
    pub size: u64,
}

pub type RenderDone = Box<dyn FnOnce(Result<Printed, PdfError>) + Send>;

#[cfg(target_os = "macos")]
mod native {
    use std::ffi::{c_char, CStr, CString};

    use super::*;

    const STATUS_OK: i32 = 0;
    const STATUS_TIMED_OUT: i32 = 1;
    const STATUS_RESOURCE_FAILED: i32 = 2;

    type Finish = extern "C" fn(*mut c_void, i32, *const c_char);

    extern "C" {
        fn chain_pdf_render(webview: *mut c_void, request_json: *const c_char, context: *mut c_void, on_finish: Finish);
        fn chain_pdf_default_paper(width: *mut f64, height: *mut f64);
    }

    /// `context` is a leaked `RenderDone`; Swift calls back exactly once.
    extern "C" fn forward_finish(context: *mut c_void, status: i32, payload: *const c_char) {
        let done = unsafe { Box::from_raw(context.cast::<RenderDone>()) };
        let payload = unsafe { CStr::from_ptr(payload) }.to_string_lossy().into_owned();
        done(match status {
            STATUS_OK => serde_json::from_str(&payload).map_err(|e| PdfError::Other(format!("unexpected render result: {e}"))),
            STATUS_TIMED_OUT => Err(PdfError::TimedOut(payload)),
            STATUS_RESOURCE_FAILED => Err(PdfError::ResourceFailed(payload)),
            _ => Err(PdfError::Other(payload)),
        });
    }

    /// Main thread, from `with_webview`: `webview` is the hidden render
    /// webview's WKWebView. `done` runs later, on the main thread.
    pub fn render(webview: *mut c_void, request: &Request, done: RenderDone) {
        let json = CString::new(serde_json::to_string(request).expect("the request serializes")).expect("JSON has no NUL");
        let context = Box::into_raw(Box::new(done));
        unsafe { chain_pdf_render(webview, json.as_ptr(), context.cast(), forward_finish) };
    }

    pub fn default_paper() -> PaperDimensions {
        let (mut width, mut height) = (0.0, 0.0);
        unsafe { chain_pdf_default_paper(&mut width, &mut height) };
        let mm = |points: f64| (points / POINTS_PER_MM * 10.0).round() / 10.0;
        if width > 0.0 && height > 0.0 {
            known_paper(PaperDimensions { width: mm(width), height: mm(height) })
        } else {
            A4
        }
    }
}

#[cfg(target_os = "macos")]
pub use native::{default_paper, render};

#[cfg(not(target_os = "macos"))]
pub fn render(_webview: *mut c_void, _request: &Request, done: RenderDone) {
    done(Err(PdfError::Unsupported("rendering PDFs isn't available on this platform yet".to_string())));
}

#[cfg(not(target_os = "macos"))]
pub fn default_paper() -> PaperDimensions {
    A4
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn resolve(options: Value) -> Result<Request, PdfError> {
        let context = RenderContext {
            out_path: "/tmp/out.pdf",
            default_paper: LETTER,
            page_url: Some("tauri://localhost/notes".to_string()),
            app_name: "mneme",
        };
        RenderOptions::parse(Some(options))?.request("<p>hi</p>".to_string(), context)
    }

    fn mm(points: f64) -> f64 {
        (points / POINTS_PER_MM * 100.0).round() / 100.0
    }

    #[test]
    fn defaults_follow_the_contract() {
        let request = RenderOptions::parse(None)
            .unwrap()
            .request(
                String::new(),
                RenderContext { out_path: "/o.pdf", default_paper: A4, page_url: None, app_name: "mneme" },
            )
            .unwrap();
        assert_eq!((mm(request.width), mm(request.height)), (210.0, 297.0));
        assert!(!request.landscape && !request.dark && request.print_background);
        assert_eq!(mm(request.margins.top), 15.0);
        assert_eq!(mm(request.margins.left), 15.0);
        assert_eq!(request.timeout_ms, 15_000.0);
        assert_eq!(request.creator, "mneme");
        assert_eq!(request.base_url, None);
    }

    #[test]
    fn falls_back_to_the_system_paper_and_the_page_url() {
        let request = resolve(json!({})).unwrap();
        assert_eq!((mm(request.width), mm(request.height)), (215.9, 279.4));
        assert_eq!(request.base_url.as_deref(), Some("tauri://localhost/notes"));
        let request = resolve(json!({ "baseUrl": "asset://localhost/" })).unwrap();
        assert_eq!(request.base_url.as_deref(), Some("asset://localhost/"));
    }

    #[test]
    fn landscape_turns_the_paper() {
        let request = resolve(json!({ "paper": "a4", "orientation": "landscape" })).unwrap();
        assert_eq!((mm(request.width), mm(request.height)), (297.0, 210.0));
        assert!(request.landscape);
        // Custom paper is taken portrait first, whichever way round it's given.
        let request = resolve(json!({ "paper": { "width": 200, "height": 100 } })).unwrap();
        assert_eq!((mm(request.width), mm(request.height)), (100.0, 200.0));
    }

    #[test]
    fn margins_take_one_number_or_some_sides() {
        let request = resolve(json!({ "margins": 20 })).unwrap();
        assert_eq!(request.margins, MarginsInPoints { top: 20.0 * POINTS_PER_MM, right: 20.0 * POINTS_PER_MM, bottom: 20.0 * POINTS_PER_MM, left: 20.0 * POINTS_PER_MM });
        let request = resolve(json!({ "margins": { "top": 25 } })).unwrap();
        assert_eq!((mm(request.margins.top), mm(request.margins.bottom)), (25.0, 15.0));
    }

    #[test]
    fn rejects_what_it_cannot_render() {
        for options in [
            json!({ "paper": "a5" }),
            json!({ "paper": { "width": 0, "height": 100 } }),
            json!({ "orientation": "sideways" }),
            json!({ "margins": -1 }),
            json!({ "margins": { "left": 120, "right": 100 } }),
            json!({ "colorScheme": "system" }),
            json!({ "timeout": 0 }),
            json!({ "timeout": 1e9 }),
            json!({ "footer": { "middle": "x" } }),
            json!({ "pageSize": "a4" }),
            json!({ "margins": { "middle": 1 } }),
            json!({ "baseUrl": "/relative/path" }),
        ] {
            assert!(matches!(resolve(options.clone()), Err(PdfError::InvalidArgument(_))), "{options}");
        }
    }

    #[test]
    fn snaps_whole_point_paper_sizes_to_a4_and_letter() {
        assert_eq!(known_paper(PaperDimensions { width: 209.9, height: 297.0 }), A4);
        assert_eq!(known_paper(PaperDimensions { width: 215.9, height: 279.4 }), LETTER);
        let a5 = PaperDimensions { width: 148.0, height: 210.0 };
        assert_eq!(known_paper(a5), a5);
    }

    #[test]
    fn serializes_what_swift_decodes() {
        let request = resolve(json!({ "footer": { "center": "Page {page} of {pages}" }, "title": "Notes" })).unwrap();
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["footer"], json!({ "center": "Page {page} of {pages}" }));
        assert_eq!(json["outPath"], "/tmp/out.pdf");
        assert_eq!(json["timeoutMs"], 15_000.0);
        assert!(json.get("header").is_none());
        assert!(json["margins"]["top"].is_f64());
    }
}
