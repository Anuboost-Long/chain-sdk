# Window Capability (`desktop.window`)

## How it works

The app window's chrome, set in the app's `package.json` for the first
frame and changeable at runtime. Requested by mneme (request 38) after
macOS 27 drew the default title bar as a solid strip over its own nav
bar; request 40 added when the window first shows.

- **Startup**: the template's `build.rs` points `CHAIN_PACKAGE_JSON` at
  the nearest `package.json`; `window.rs` compiles it in, and
  `window::setup` applies `"chain.window"` to the windows Tauri created
  before the first frame. With no options nothing is touched.
- **Options** (parsing, defaults, merging, what each platform does) are
  `chain_core::window::Chrome`. Applying them on macOS: style mask,
  transparency, title and button visibility through objc2
  (`window.rs`); the bar size (an empty `NSToolbar`, so AppKit lays out
  the bar and buttons) and a custom button position (re-applied inside
  AppKit's own layout pass) in `crates/core/swift/ChainWindow.swift`.
  Appearance and background colour are Tauri calls; the WKWebView's own
  background is turned off while a colour is set.
- **First show** (`showWhen`, `showTimeout`): startup-only keys,
  `chain_core::window::FirstShow`, stripped before the chrome is parsed
  and refused by `setOptions()`. Unless `immediately`, `setup` makes
  the window undrawn (`set_undrawn`: alpha 0, clicks pass through) —
  not ordered out, because WebKit stops rendering an ordered-out window
  and the app must launch as usual. Showing restores alpha in place, so
  focus and order are a normal launch's. `Tracked.awaiting` holds what
  the window waits for; a timer thread, `window_show`, or the SDK's
  `window_page_painted` (two animation frames after it loads, on every
  page load) show it once, emitting `chain://window-shown`. Reloads
  find `awaiting` empty and change nothing.
- **Drag regions**: the SDK (`packages/sdk/src/window.ts`) measures
  `data-chain-drag-region` elements and their holes (interactive
  elements, `="false"`) and sends them on every layout change. On macOS
  `ChainDragView`, a transparent view over the webview, takes presses
  there and calls `performDrag(with:)` with the real event; double-click
  follows the System Settings choice. Windows/Linux: the SDK's mousedown
  calls `window_start_drag` / `window_title_bar_double_click`.
- **Insets and full screen**: measured on window events
  (`on_window_event`) and option changes, emitted as
  `chain://window-insets` / `chain://window-full-screen` to that window.
  The SDK mirrors them into `--chain-*` CSS variables and
  `data-chain-full-screen` on `<html>`.

## How to use it

```json
// package.json
"chain": { "window": {
  "titleBarStyle": "overlay",
  "titleBarSize": "large",
  "appearance": "dark",
  "backgroundColor": "#101418"
} }
```

```tsx
import { desktop } from "@chain/sdk";

<nav data-chain-drag-region
     style={{ height: 52, paddingLeft: "calc(var(--chain-title-bar-inset-left, 0px) + 12px)" }}>
  <button>Still clickable</button>
</nav>

await desktop.window.setOptions({ appearance: "light", backgroundColor: "#f7f5f0" });
const off = desktop.window.onFullScreenChange((full) => { /* ... */ });
```

Show the window once the page has drawn (no white frame before a dark
launch screen), or when the app says so:

```json
"chain": { "window": { "showWhen": "firstPaint", "showTimeout": 3000 } }
```

```ts
// "showCalled": show once the data is in; calling it again does nothing.
await desktop.window.show();
// Start a launch animation when the window is actually visible.
if (await desktop.window.isShown()) start();
else desktop.window.onShown(start);
```

Restart `chain dev` after editing `package.json`'s options. Prefer
`titleBarSize` over `windowButtons.position` for a native feel. Full
semantics: `CONTRACT.md`; per-platform support: `availability()`.

## Files to check

- `capabilities/window/contract.ts`, `agent-docs/capabilities/window/CONTRACT.md`
- `crates/core/src/window.rs` — options, availability, insets, objc2 calls, FFI to Swift
- `crates/core/swift/ChainWindow.swift` — drag view, title bar size, button placer
- `crates/core/build.rs` — compiles the Swift file, links AppKit
- `packages/cli/templates/window.rs` (copy: `apps/playground/src-tauri/src/window.rs`) — Tauri commands, setup, window events
- `packages/cli/templates/build.rs`, `lib.rs` — `CHAIN_PACKAGE_JSON`, wiring
- `packages/sdk/src/window.ts` — API, drag-region measuring, CSS variables, the first-paint signal
- `research/MACOS.md` (measurements, what didn't work), `research/WINDOWS.md`
- `apps/playground/index.html` + `src/launchScreen.ts` — a worked launch screen: inline HTML/CSS so it's the first frame with `firstPaint`, progress animating only after `onShown`, dismissed once the app has loaded
