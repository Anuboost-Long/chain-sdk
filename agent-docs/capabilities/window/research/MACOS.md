# Window — macOS research

Verified on macOS 27.0 (Apple Silicon), Tauri 2.11.5 / tao 0.35.3 /
wry 0.55.1, in `apps/playground` (dev build), 6 October 2026.

## What maps to what

| Option | AppKit | Where |
| --- | --- | --- |
| `overlay`, `hidden` | `styleMask` + `.fullSizeContentView` | `window.rs` (objc2) |
| `transparent`, `overlay`, `hidden` | `titlebarAppearsTransparent = true` | `window.rs` |
| `titleVisible` | `titleVisibility` | `window.rs` |
| button visibility (`hidden` = all hidden) | `standardWindowButton(_:).isHidden` | `window.rs` |
| `titleBarSize` | an empty `NSToolbar` + `toolbarStyle` `.unifiedCompact` / `.unified` | `ChainWindow.swift` |
| `windowButtons.position` | button/container frames, re-applied on frame-change notifications | `ChainWindow.swift` |
| `appearance` | Tauri `set_theme` → `NSWindow.appearance` (app-wide on macOS) | template |
| `backgroundColor` | Tauri `set_background_color` + WKWebView `drawsBackground = NO` | template + `window.rs` |
| drag regions, double-click | `ChainDragView` over the WKWebView, `performDrag(with:)` | `ChainWindow.swift` |
| insets | `frame.height − contentLayoutRect.maxY`; buttons via `convert(_:to: nil)` | `window.rs` |
| full screen | `styleMask.contains(.fullScreen)` / Tauri `is_fullscreen` on `Resized` | `window.rs` + template |

`hidden` is overlay with every button hidden, not `decorations: false`:
a borderless window loses its rounded corners, shadow and resize edges.

## Measured layouts (macOS 27)

| Bar | Height | Close button left | Close top | Button size | Spacing |
| --- | --- | --- | --- | --- | --- |
| standard title bar | 32 | 9 | 9 | 14 | 23 |
| compact toolbar (`medium`) | 40 | 12 | 13 | 14 | 23 |
| unified toolbar (`large`) | 52 | 19 | 19 | 14 | 23 |

## Findings

- **Tauri has no runtime traffic-light setter.** Only the builder's
  `traffic_light_position` (tao re-applies it in its view's `drawRect:`,
  forever, so a runtime change would fight it). Chain never uses it and
  places the buttons itself.
- **Moving buttons after AppKit's layout looks wrong.** AppKit lays them
  out again on resize, zoom, focus and full-screen changes; moving them
  back from a Tauri `Resized` handler showed them jump on double-click
  zoom (reported by the user). `ChainWindow.swift`'s `ButtonPlacer`
  listens to the buttons' and their container's
  `NSView.frameDidChangeNotification` instead: it's posted inside the
  same layout pass, so they're back before anything draws. Spacing is
  captured once, before the first move — measured mid-relayout, one
  button had moved back and another hadn't (width went 60 → 50).
- **A hand-picked position doesn't feel native** (user feedback: "size
  or spacing"). A toolbar makes AppKit itself lay out a taller bar —
  hence `titleBarSize`, the recommended option.
- **A toolbar created before the window shows reported a 66 pt bar**
  (52 + a 14 pt label row) even though the buttons were laid out for
  52. `displayMode = .iconOnly` fixes it.
- **Resetting `position` to `null`**: `setNeedsLayout` /
  `layoutSubtreeIfNeeded` on the theme frame doesn't put the buttons
  back; rebuilding the title bar does (a toolbar set and restored).
- **Hit-testing in the enlarged bar**: with overlay, points in the
  title bar area go to the WKWebView (or `ChainDragView`), the three
  buttons to `_NSTheme*Widget` — also with the toolbar attached and the
  container enlarged by `position`. Probed with `hitTest:` on the theme
  frame.
- **Dragging over IPC doesn't work reliably.** Tauri's `start_dragging`
  (tao `drag_window`) calls `performWindowDragWithEvent:` with
  `NSApp.currentEvent`; when the page's invoke arrives that's usually no
  longer the mouse-down, and the drag silently doesn't start (reported
  by the user). `ChainDragView` gets the real mouse-down from AppKit:
  `hitTest:` claims a point only inside a region and outside its holes,
  so everything else still reaches the page. `acceptsFirstMouse` true,
  as a title bar.
- **Double-click setting**: `AppleActionOnDoubleClick` in the global
  domain — `Maximize` (default; absent on this machine), `Minimize`,
  `None`, `Fill`; the older boolean `AppleMiniaturizeOnDoubleClick`
  when absent. `Fill` zooms.
- **WKWebView paints its own background** (white, or dark grey in dark
  appearance) over the window's colour — Tauri: "macOS: not implemented
  for the webview layer". `drawsBackground = NO` (KVC, the key wry sets
  for transparent windows; not public API) while a background colour is
  set; with the page made transparent the window colour showed through.
- **The SDK can't use `requestAnimationFrame` to batch region updates**:
  WebKit pauses frames while the window is covered, so regions stopped
  following the layout. A microtask instead.
- Sync Tauri commands run on the main thread, and `run_on_main_thread`
  runs inline there; AppKit can send window events synchronously while
  options change, so the state lock is never held across AppKit calls.

## Not verified

- A physical drag and double-click on `ChainDragView` (no Accessibility
  permission for synthetic clicks in the session; routing verified by
  hit-testing, double-click by calling the same action).
- Entering/leaving full screen with real events, `transparent`, `hidden`
  and the theme switch in mneme's own window.
- Older macOS versions (layouts above are macOS 27's).
