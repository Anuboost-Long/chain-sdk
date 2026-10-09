# PageZoom Capability (`desktop.pageZoom`)

## How it works

`set(factor)` calls Tauri's `Webview::set_zoom` on the calling page's
webview, which is WKWebView's `pageZoom` on macOS. That's real page zoom:
layout reflows at the new size and `devicePixelRatio` scales, so canvases
stay sharp. The range check is `chain_core::page_zoom::check` (0.25–5).
The factor is remembered per webview in `PageZoomState`
(`templates/lib.rs`) for `get()`, since WebKit's isn't readable through
Tauri. It survives page reloads, not app restarts.

## How to use it

```ts
import { desktop } from "@chain/sdk";

const STEPS = [0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2];

await desktop.pageZoom.set(savedFactor); // at startup, before the window shows
const current = await desktop.pageZoom.get();
const next = STEPS.find((step) => step > current) ?? current;
await desktop.pageZoom.set(next); // Cmd +
```

`INVALID_ARGUMENT` outside 0.25–5 or for a non-finite number.

## Files to check

- `agent-docs/capabilities/page-zoom/CONTRACT.md` — what "zoom" means
  here, and the non-goals.
- `capabilities/page-zoom/contract.ts` — the types.
- `crates/core/src/page_zoom.rs` — the accepted range.
- `packages/cli/templates/lib.rs` (and the playground copy) —
  `page_zoom_set`/`page_zoom_get` and `PageZoomState`.
- `packages/sdk/src/page-zoom.ts` — the SDK wrapper, with its NaN check.
- `agent-docs/capabilities/page-zoom/research/MACOS.md` — the
  measurements.
