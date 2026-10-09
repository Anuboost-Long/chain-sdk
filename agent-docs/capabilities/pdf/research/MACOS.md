# PDF — macOS research

macOS 27.0.1, WebKit as shipped, Swift 6.4, 8 October 2026. Measured
with standalone Swift spikes (a WKWebView in an off-screen window) and
then in `apps/playground` through the dev inspector.

## Engine: WKWebView's print path

- `WKWebView.createPDF(configuration:)` (macOS 11) makes **one tall
  page** — no pagination. Not usable.
- `WKWebView.printOperation(with: NSPrintInfo)` (macOS 11) paginates
  like printing does. Saving instead of printing:
  `NSPrintInfo(dictionary: [.jobDisposition: .save, .jobSavingURL: url])`,
  `showsPrintPanel = false`, `showsProgressPanel = false`.
  - `operation.view?.frame = webView.bounds` is required, and it has to
    run with `runModal(for: window, delegate:didRun:)`; plain
    `runOperation()` gives blank pages (widely reported, matches here).
  - The window can be one that was never shown (a hidden Tauri window).
- Measured on that path:
  - `break-before: page` and `break-inside: avoid` work.
  - `@page { margin: 30mm }` wins over NSPrintInfo's margins (content
    started at exactly 85.04 pt).
  - `@page { size }` is ignored: the size is NSPrintInfo's `paperSize`.
  - No `@page` margin boxes (`@bottom-center { content: counter(page) }`
    drew nothing). Header/footer are drawn afterwards instead.
  - Landscape: `orientation = .landscape` with the paper given wide
    gives 842 × 595 pages.
  - Web links come out as PDF link annotations.
  - **Scale**: WebKit prints 1 CSS px as 0.8 pt (a 400 px block was
    320 pt), not CSS's 0.75. `NSPrintInfo.scalingFactor = 0.9375` fixed
    the size but **broke pagination** (forced breaks and
    `break-inside: avoid` stopped working). `:root { zoom: 0.9375 }`
    injected before printing gives 299 pt for 400 px with pagination and
    `@page` margins intact — that's what ships, so sizes match Chromium
    (and WebView2 on Windows).
  - Backgrounds: off by default. Both `WKPreferences.shouldPrintBackgrounds`
    (public from macOS 13.3) and CSS `print-color-adjust: exact` turn
    them on. Setting the preference by KVC on a fresh configuration
    didn't. Ships as injected CSS, which works on every version.
  - `webView.appearance = NSAppearance(named: .aqua)` makes
    `prefers-color-scheme` light even when the app is dark.
- Loading through the app's own schemes: wry's `WKURLSchemeHandler`
  (wry 0.57 `url_scheme_handler.rs` `start_task`) treats the webview
  argument as a `WryWebView` and reads its ivars, so a plain WKWebView
  can't borrow the app's handlers — it would crash. The render webview
  is therefore a hidden Tauri window (`visible(false)`), which has
  `tauri://`/dev-server and `asset://` like the app.
- Scripts off: Tauri's `disable_javascript()` sets
  `defaultWebpagePreferences.allowsContentJavaScript = false` (wry
  0.55 `wkwebview/mod.rs`), which leaves app-evaluated JS working. The
  readiness check uses `callAsyncJavaScript(..., in: .defaultClient)`.
  Confirmed: an inline `<script>` in the document didn't run.
- Load detection: KVO on `isLoading` (wry owns the navigation delegate).

## Post-processing

The printed PDF is redrawn page by page into a new `CGContext` PDF
(`drawPDFPage`), which keeps text as text with embedded fonts, and adds:

- header/footer text with CoreText (`CTLineDraw`), `{page}`/`{pages}`
  substituted — the page count is known only now;
- metadata through the context's auxiliary info (Title, Author,
  Creator);
- the link annotations, which `drawPDFPage` drops: read from the page
  dictionary (`Annots` → `Link` with an `A`/`URI` action) and re-added
  with `CGPDFContextSetURLForRect`. Internal (`#anchor`) links aren't
  carried over.

## Complex scripts don't copy or search (OS limitation)

Khmer `ខ្ញុំស្រលាញ់ភាសាខ្មែរ សួស្តី` extracts (PDFKit, i.e. Preview) as
`ខ"#$%ស'(ញ*+(,(-ខ.រ ស0ស12`. Not WebKit's fault: the same string drawn
straight into a Quartz PDF context with `CTLineDraw` gives the same
result in Khmer Sangam MN, Khmer MN and the system font. The
`ToUnicode` CMap Quartz writes has entries only for glyphs the font's
`cmap` maps to a single character (here 4 of them); stacked subscripts,
vowel signs and ligatures have none. Same for Arabic (joined forms) and
Devanagari (conjuncts); Chinese and Latin (including fi/ffi ligatures)
extract correctly.

Fixes looked at and not taken:

- Rewriting `ToUnicode` maps would need the subset font's glyph →
  original glyph mapping and a shaping-derived glyph → text table, and
  still can't fix reordered vowels (visual order ≠ logical order).
- `/ActualText` spans (Quartz has `CGPDFContextBeginTag` with
  `kCGPDFTagPropertyActualText`) would need us to draw the text runs
  ourselves; WebKit draws them.
- An invisible text layer would duplicate every complex-script word in
  copy/search results.

Reported as `complexScriptSearch: false`. On Windows, WebView2's
Chromium/Skia PDF backend writes correct `ToUnicode`/`ActualText`, so it
should be true there (verify).

## Default paper

`NSPrintInfo.shared.paperSize` — the "Default paper size" in Printers &
Scanners, which itself follows the region. Whole points (A4 = 595 × 842)
so it's snapped to A4/Letter within 1 mm.

## Verified (playground, 8 October 2026)

- `availability()`: everything true but `cssPageSize`,
  `cssPageMarginBoxes`, `complexScriptSearch`; `defaultPaper` A4.
- Render with an `asset://` image from `files.url()`, the app's
  `/src/App.css`, Khmer, Chinese, a link, cards with backgrounds: 3 A4
  pages, header and "Page n of 3" footer on each, Title/Author/Creator
  set, link clickable, backgrounds printed, light colours, inline script
  not run, 341 ms.
- Letter landscape: 792 × 612. Missing image → `NOT_FOUND` naming the
  URL; `paper: "a5"` → `INVALID_ARGUMENT`; a server that never answers →
  `TIMEOUT` after 2 s. No file left behind in each failure.
