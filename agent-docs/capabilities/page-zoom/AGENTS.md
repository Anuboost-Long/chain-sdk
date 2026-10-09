# PageZoom Capability — Agent Memory

Scope: `desktop.pageZoom.set(factor)` / `get()` — browser-style zoom of
the calling window's page. Requested by Lazify (request 08 in
`lazify-chain/docs/chain-sdk-requests/`).

## What's already decided

- **Its own capability, not part of `window`**: `window` is the chrome
  around the page (title bar, buttons, insets). Its CONTRACT.md's line
  "page zoomed with the webview's zoom isn't supported" refers to window
  insets under zoom and stays true there. (The window files were staged
  for a commit by someone else when this was added, so they were left
  untouched.)
- **The webview's own page zoom (`Webview::set_zoom`)**, never CSS
  `zoom`: it reflows and raises `devicePixelRatio`, which xterm.js needs.
- **0.25–5**, browsers' range; Lazify clamps to its own 0.5–2 steps.
- **The factor is remembered per webview label** in `PageZoomState`
  (`templates/lib.rs`), because Tauri has no getter.
- **No pinch event**: WKWebView pinch is magnification, which is off.

## Status

**Verified on macOS** in a throwaway app through the SDK: reflow and
`devicePixelRatio` at 1, 1.5, 0.75 and 1.25 (see `research/MACOS.md`);
the factor survives a reload; `0.1` and `NaN` reject `INVALID_ARGUMENT`.
Unit test for the range in `crates/core/src/page_zoom.rs`.

- In Lazify itself (reported by its session after `chain update`): at
  0.75 / 1.5 / 1, `innerWidth` went 2016 / 1008 / 1512 and
  `devicePixelRatio` 1.5 / 3 / 2. Its terminal uses xterm.js's DOM
  renderer, and text stays sharp. Lazify also noted that xterm doesn't
  recompute its internal device-pixel cell size when `devicePixelRatio`
  changes. That would only show with xterm's canvas or WebGL renderer.

## What's NOT done yet

- [ ] An app using xterm's canvas/WebGL renderer may need to refit the
      terminal after `set()`. That's an app-level concern, untested.
- [ ] Windows: `research/WINDOWS.md`.
- [ ] Contract tests under `capabilities/page-zoom/tests/`.
