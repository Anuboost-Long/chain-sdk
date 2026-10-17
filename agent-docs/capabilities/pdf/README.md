# PDF Capability (`desktop.pdf`)

## How it works

`render(html, options)` → `pdf_render` (template `pdf.rs`):

1. `chain_core::pdf::RenderOptions` parses and checks the options
   (`INVALID_ARGUMENT` before anything is created) and turns them into a
   `Request` in points.
2. The template opens a hidden Tauri window `chain-pdf-<n>` on
   `about:blank` (scripts off unless `runScripts`), and in its
   `with_webview` hands the WKWebView to `chain_core::pdf::render`.
3. `crates/core/swift/ChainPdf.swift` loads the HTML with the base URL
   (the caller's page URL by default), waits for `isLoading` to end,
   runs a readiness script in its own content world (injects the zoom
   and `print-color-adjust` CSS, waits for images, stylesheets and
   fonts, reports failures), prints with `printOperation` to
   `<out>.print`, then redraws that into `<out>` adding header/footer,
   metadata and links. A timer rejects `TIMEOUT` until printing starts.
4. The template destroys the window and `files.adopt`s the PDF into the
   app's files, returning `{ reference, pageCount, size }`.

`availability()` → `chain_core::pdf::availability()`, with the default
paper from `NSPrintInfo`.

## How to use it

```ts
import { desktop } from "@chain/sdk";

const { reference, pageCount } = await desktop.pdf.render(html, {
  footer: { left: "Made with mneme", right: "Page {page} of {pages}" },
  title: "Cell biology summary",
});
await desktop.share.show([{ reference, name: "Cell biology summary" }], { anchor });
```

`html` is a whole document. Use `break-inside: avoid` on cards/rows and
`break-after: avoid` on headings. Check `availability()` to disable the
menu item where `available` is false.

## Files to check

- `capabilities/pdf/contract.ts`, `agent-docs/capabilities/pdf/CONTRACT.md`
- `crates/core/src/pdf.rs` — options, availability, FFI to Swift
- `crates/core/swift/ChainPdf.swift` — load, readiness, print, redraw
- `crates/core/build.rs` — compiles it, links WebKit and CoreText
- `packages/cli/templates/pdf.rs` (copy: `apps/playground/src-tauri/src/pdf.rs`) — hidden window, adopt into files
- `packages/cli/templates/lib.rs` — wiring, `release_abandoned_work` skips `chain-pdf-`
- `packages/sdk/src/pdf.ts` — API and error codes
- `research/MACOS.md` — measurements and the complex-script limitation
