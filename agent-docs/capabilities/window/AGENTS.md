# Window Capability — Agent Memory

Scope: `desktop.window` — the app window's chrome (title bar style and
size, title text, window buttons, appearance, background colour),
page-declared drag regions, title bar insets and full-screen state.
Requested by mneme (request 38,
`docs/chain-sdk-requests/38-window-title-bar.md` in the mneme repo);
when the window first shows by request 40
(`40-show-window-when-ready.md`).

## What's already decided

- **Startup options are package.json's `"chain": { "window": ... }`**,
  compiled in (template `build.rs` sets `CHAIN_PACKAGE_JSON`;
  `window.rs` `include_str!`s it) and applied in setup, before the first
  frame. No options → nothing applied at all. Invalid options stop the
  app at startup with the key named.
- **`titleBarSize` is the native path**: an empty toolbar so AppKit
  sizes the bar and places the buttons. `windowButtons.position` is the
  escape hatch; recommend the size first (the user found the hand-placed
  position un-native).
- **AppKit work that subclasses or observes views is Swift**
  (`crates/core/swift/ChainWindow.swift`): drag view, button placer,
  toolbar. One-shot property sets are objc2 in `window.rs`.
- **Drag regions are native on macOS**; the SDK only measures and sends
  rectangles. Never go back to starting drags over IPC there (see
  research/MACOS.md). Windows/Linux use the SDK's mousedown →
  `window_start_drag`.
- `hidden` on macOS = overlay with every button hidden, never
  `decorations: false`.
- `null` resets any field to its default; runtime changes don't persist.
- **A window waiting to show is undrawn (alpha 0, click-through), never
  ordered out or Tauri-hidden**: WebKit stops rendering an ordered-out
  window (no first paint would ever come), and an undrawn window keeps
  the launch normal — key, focused, one window for Dock clicks.
  Showing restores alpha only, so it never reorders or activates.
- `showWhen`/`showTimeout` are startup-only; `setOptions()` refuses
  them. Only the first appearance: `awaiting` is cleared once and
  reloads never set it again.
- Don't grow this into window size/position management, toolbars with
  items, vibrancy, or drawn replacement buttons (CONTRACT.md non-goals).

## Status

- [x] macOS 27.0, `apps/playground` dev build through the dev inspector,
      6 October 2026:
      - no options → `standard`, insets all zero, unchanged window.
      - package.json overlay + `large` + dark + background colour →
        applied on the first frame; insets `{height: 52, left: 79,
        windowButtons: {x: 19, y: 19, width: 60, height: 14}}`.
      - runtime `titleBarSize` medium/large/standard → 40/52/32, buttons
        at AppKit's spot each time; zoom/unzoom ×2 → no movement.
      - custom position `{24, 30}` through zoom ×4 → unchanged; `null`
        → back to AppKit's 19/19.
      - hit-testing: drag areas → `ChainDragView`, page button and page
        → webview, window buttons → their widgets; regions follow a
        layout change.
      - SDK region logic (synthetic events, Windows/Linux path):
        region/text drag, button/`="false"`/body don't, double-click
        once on mouse-up, cancelled when moved.
      - double-click action toggles zoom; background colour shows
        through a transparent page.
- [x] Propagated to mneme with `chain update` (lib.rs, build.rs,
      window.rs; second run "Already up to date"); native `cargo check`
      and `tsc` clean. mneme hasn't set any options yet.

- [x] `showWhen` (request 40), macOS 27.0, `apps/playground` dev build,
      7 October 2026, timestamps from temporary probes (reverted):
      - `firstPaint`: page painted while undrawn, shown 292 ms after
        setup by the paint signal, not the timeout.
      - `showCalled` + 4000/6000 ms: page painted while undrawn, timer
        showed it at +4.005 s / +6.005 s. Screenshot while waiting: app
        active in the menu bar, no window drawn, clicks reaching the
        window behind; after: window drawn, in front, key.
      - `isShown()` true after; `show()` twice resolves; page reload
        doesn't hide it (`window_page_painted` → awaiting None);
        `setOptions({ showWhen })` → `INVALID_ARGUMENT`;
        `availability().showWhen` all true.
      - One run stalled ~2 minutes; the probe's own `eprintln!` was
        blocked on the `tauri dev` output pipe (lines arrived ~116 s
        after their timestamps). The real code writes nothing there;
        rerun twice, clean.
      - Launch screen in the playground, frame by frame: first visible
        frame already the dark launch screen, no white frame.
- [x] `showWhen` propagated to mneme with `chain update`, 7 October 2026
      (lib.rs, window.rs); native `cargo check` and `tsc` clean. The
      update conflicted on `Cargo.toml`: the template pins `chain-core`
      to a git rev older than this change, so mneme keeps its local path.

## Not done yet

- [ ] `showWhen`: a person watching a real launch for a white frame
      (mneme, dark theme); Dock click while waiting; switching to
      another app while waiting (should appear behind, unfocused); Vite
      hot reload in `chain dev`; launch with the window covered (WebKit
      may pause frames → the timeout shows it).

- [ ] Physical drag/double-click confirmed by a person (in progress).
- [ ] Full screen with real events: `onFullScreenChange`, buttons inset
      `null`, CSS variable to 0, position restored after leaving.
- [ ] `transparent` and `hidden` looked at; runtime theme switch in
      mneme.
- [ ] Windows and Linux — research/WINDOWS.md checklist.
- [ ] Contract tests under `capabilities/window/tests/`.
