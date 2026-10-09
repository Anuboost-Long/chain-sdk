# PageZoom Capability — Contract

## What this is

Browser-style page zoom for the calling window's page. Driven by Lazify's
request 08 (its zoom setting and Cmd +/−/0). This is the webview's own
page zoom (WKWebView `pageZoom` on macOS), not CSS `zoom`:

- **Layout reflows** at the new size. The page sees a narrower or wider
  viewport in CSS pixels (`innerWidth` 800 → 533 at 1.5 → 1066 at 0.75,
  verified).
- **`devicePixelRatio` scales with it** (2 → 3 at 1.5 on a Retina
  display), so canvases that size their backing store by
  `devicePixelRatio`, such as xterm.js, stay sharp. CSS `zoom` breaks
  exactly this.
- `getBoundingClientRect`, fixed-position overlays and drawing agree,
  because it's all one coordinate system.
- A library that caches device-pixel sizes must notice the change itself.
  xterm.js's canvas/WebGL renderers don't recompute their cell size when
  `devicePixelRatio` changes; its DOM renderer is unaffected.

## `set(factor)` and `get()`

- `set(factor)` zooms the calling window's page. `1` is 100%. Accepted
  from **0.25 to 5** (browsers' own range). Anything else, including
  `NaN` and `Infinity`, rejects `INVALID_ARGUMENT`. Lazify's 0.5–2 steps
  sit inside that range, and step policy is the app's.
- `get()` returns the factor last set for this window's page, `1` if
  never set.
- The zoom **survives a reload** of the page (WebKit keeps it on the
  webview), and `get()` agrees afterwards. It does **not** survive an app
  restart: the app saves its factor and calls `set()` at startup.
- To avoid a frame at 100% on launch, set it before the window first
  shows, with `desktop.window`'s show-when-ready option.

## Errors

- `INVALID_ARGUMENT` — outside 0.25–5, or not a finite number.
- `UNSUPPORTED` — outside a Chain runtime.
- `NATIVE_FAILURE` — the webview refused.

## Non-goals

- **No pinch-gesture event.** Trackpad pinch on a WKWebView is
  *magnification* (a visual scale, not reflow), which Tauri leaves off.
  Nothing reports it, and Lazify listed the event as optional.
- **No persistence across restarts** — the app owns its setting.
- **No app menu** (View → Zoom In etc.) — a later request.
- **No zoom for other webviews** (the `browser` window, PDF rendering).
