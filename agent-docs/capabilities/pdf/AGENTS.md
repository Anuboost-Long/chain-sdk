# PDF Capability — Agent Memory

Scope: `desktop.pdf` — an HTML string rendered off-screen into a
paginated PDF in the app's files. Requested by mneme (request 41,
`docs/chain-sdk-requests/41-render-pdf.md` in the mneme repo) for
"Share as PDF" (its `docs/features/45-share-as-pdf.md`).

## What's already decided

- **The engine is the app's own webview engine**, through WebKit's print
  path (`printOperation`), not `createPDF` (one tall page) and never a
  JS PDF library. See research/MACOS.md.
- **The render webview is a hidden Tauri window** (`chain-pdf-<n>`), not
  a WKWebView made in Swift: wry's scheme handlers can't serve a webview
  wry didn't make, and the document must load the app's `asset://` and
  bundled files. Its label prefix keeps it out of `release_abandoned_work`
  (a render page load must not roll back storage or stop a recording).
- **Output is a `desktop.files` reference**, never a path (files'
  non-goal). Swift writes to the cache dir; the template `adopt`s the
  finished file, so a failure never leaves a reference or a half file.
- **Header/footer are redrawn afterwards** with CoreText (WebKit has no
  margin boxes); this is also where metadata and links are set.
- **`:root { zoom: 0.9375 }`** makes 1 CSS px print as 0.75 pt like
  Chromium. Don't switch to `NSPrintInfo.scalingFactor` — it breaks
  pagination.
- **Complex-script copy/search is an OS limitation**, reported as
  `complexScriptSearch: false`, not hidden. Don't promise mneme it works.
- One render at a time (`PdfState.rendering`).
- `TIMEOUT` was added to `ChainErrorCode` for this.

## Status

- [x] macOS 27.0.1, `apps/playground` dev build, 8 October 2026 — see
      research/MACOS.md "Verified": pages, breaks, header/footer page
      numbers, metadata, links, backgrounds, light scheme, scripts off,
      asset-protocol image and app CSS, landscape Letter, NOT_FOUND /
      INVALID_ARGUMENT / TIMEOUT, nothing left behind.
- [x] `cargo test -p chain-core --lib pdf::` (option parsing/validation).
- [x] Propagated to mneme with `chain update`, 8 October 2026 (lib.rs
      merged, pdf.rs added; third run "Already up to date"); its native
      `cargo check` clean.

## Not done yet

- [ ] mneme: a real Share as PDF run in its window.
- [ ] `runScripts: true` exercised (only the default was).
- [ ] Lazy `<img loading="lazy">` below the first page, web fonts
      (`@font-face` from the app bundle) failing/succeeding.
- [ ] A long document (60-card deck) timing and memory.
- [ ] Windows — research/WINDOWS.md. Linux not planned.
- [ ] Contract tests under `capabilities/pdf/tests/`.
