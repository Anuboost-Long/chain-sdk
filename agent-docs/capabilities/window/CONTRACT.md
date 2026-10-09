# Window — Semantic Contract

`desktop.window` lets an app decide what its window's chrome looks like
— the title bar, the window buttons, the background colour — mark parts
of its page as places to drag the window from, and know where the system
chrome sits over its page. Structural contract:
`capabilities/window/contract.ts`.

Requested by mneme (request 38): on macOS 27 the default title bar draws
as a solid strip above mneme's own dark nav bar, and an app may not
touch Tauri's window options itself. Request 40 added when the window
first shows: a dark page showed a white frame (the startup background)
before its launch screen.

## Defaults: today's window

Every option has a default that keeps the standard system title bar.
With no options at all, nothing is applied and the window is exactly
what the OS draws by default.

## Options

The same `WindowOptions` shape is set two ways:

- **At startup**, in the app's `package.json`:
  `"chain": { "window": { "titleBarStyle": "overlay", ... } }`. Compiled
  into the app and applied before the window's first frame, so there's
  no flash of the standard bar or of a white background. An invalid
  value (unknown key, unknown style, malformed colour, negative
  position or timeout) stops the app at startup with a message naming the key —
  `chain dev` shows it on the first run.
- **At runtime**, `setOptions(options)` on the window the calling page
  is in. Only the fields given change; `windowButtons` merges field by
  field; `null` puts any field back to its default. Runtime changes last
  until the app quits — the next launch starts from `package.json`
  again. An invalid value rejects `INVALID_ARGUMENT` and changes
  nothing.

`options()` returns what's in effect (`ResolvedWindowOptions`):
defaults filled in, and options this platform can't do shown as what
actually applies (e.g. `overlay` on Windows reads back `standard`).

### `titleBarStyle`

| Style | Title bar | Window buttons | Page starts |
| --- | --- | --- | --- |
| `standard` (default) | system, system colours | in the bar | below the bar |
| `transparent` | system height, drawn in the window's background colour | in the bar | below the bar |
| `overlay` | none drawn; the page shows through | over the page | at the window's top |
| `hidden` | none | none | at the window's top |

`hidden` keeps the window's rounded corners, shadow and resizing; the
app draws its own close/minimise/zoom controls if it wants any.

### `titleBarSize`

`"standard"` (default), `"medium"` or `"large"`: the bar heights macOS
lays out itself, window buttons placed and centred by the OS — 32, 40
and 52 px on macOS 27 (a standard title bar, a compact toolbar's bar, a
toolbar's bar). This is the native choice for an app bar the buttons sit
on: the buttons never move unexpectedly and match every other app. The
heights differ between macOS versions, so an app sizes its bar from
`insets().height` rather than hardcoding them. No effect with `hidden`;
ignored on Windows and Linux.

### `titleVisible`

The window's title in the bar. Default: shown for `standard` and
`transparent`, hidden for `overlay` (`true` draws it over the page,
centred in the bar's area). Never shown with `hidden`.

### `windowButtons`

- `close`, `minimize`, `zoom` (default `true`): show or hide each
  button. A hidden button keeps its slot, as AppKit does — hiding
  `zoom` (the last) leaves no gap.
- `position` (default `null`: where AppKit puts them for the bar's
  size): `{ left, centerY }` in CSS pixels — the first button's left
  edge from the window's left, and the buttons' vertical centre from the
  window's top. For an exact position the sizes above don't give; prefer
  `titleBarSize` for a native look. Kept through resizing, zoom, focus
  changes and leaving full screen, with no visible jump; the OS's
  spacing between buttons is kept. Applies with any style that shows
  buttons.

### `appearance`

`"system"` (default), `"light"` or `"dark"`: the title bar's look, so
title text and buttons stay readable on the app's own colours. It's the
whole window's appearance, so the page's `prefers-color-scheme` follows
it too — an app with its own theme switch sets it to match.

### `backgroundColor`

The window's own colour (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`): what
shows before the page paints, behind a transparent page, and in a
`transparent` title bar. Default `null`: the OS default. The page's own
background still paints over it.

## When the window first shows

Two startup-only keys in package.json's `"chain.window"`
(`WindowStartupOptions`). `setOptions()` rejects them with
`INVALID_ARGUMENT`: they decide the very first moment, so runtime is too
late.

- **`showWhen`**
  - `"immediately"` (default): today's window — it appears as the app
    starts, before the page has drawn anything.
  - `"firstPaint"`: the window stays hidden until the page has drawn
    its first frame, then appears already drawn. "Drawn" means the SDK
    has loaded in the page and two animation frames have passed, so
    the first thing seen is the page's own content — HTML and CSS
    inline in `index.html` (a launch screen) is on screen by then. A
    page that never imports `@chain/sdk` waits for the timeout.
  - `"showCalled"`: the window stays hidden until the page calls
    `show()` — for an app that waits for its own data or a particular
    element.
- **`showTimeout`**: milliseconds (default 3000) after which the window
  appears anyway, whatever `showWhen` says, so a page that fails to load
  or never calls `show()` still gets a window. `0` or more; `null` → the
  default. Ignored with `immediately`.

While hidden:

- The app is launched as usual: Dock icon (or taskbar entry), menu bar,
  and the window is already the key, focused window at its normal size
  and position — it just isn't drawn and lets clicks through. Clicking
  the Dock icon then does nothing (there's still exactly one window).
- The page runs normally: scripts, timers and animation frames.

When it appears, it's exactly where a normal launch put it: in front
and focused if the app is still active, behind if the user switched to
another app meanwhile — showing never takes focus from another app.

- **`show()`** shows the window if it hasn't appeared yet; otherwise it
  does nothing and resolves. Safe in any mode, any number of times.
- **`isShown()`**: whether the window has appeared. `true` with
  `immediately`; once `true` it stays `true` (minimising, full screen
  and hiding the app don't change it).
- **`onShown(listener)`** fires once, when it appears. It doesn't fire
  for a window that's already shown, so check `isShown()` first:

  ```ts
  if (await desktop.window.isShown()) start();
  else desktop.window.onShown(start);
  ```

Only the first appearance counts: reloading the page, or a hot reload
in `chain dev`, never hides the window again.

## Dragging the window

- **`data-chain-drag-region`** on an element makes it, and everything
  inside it, drag the window on a primary-button press — except
  interactive elements (buttons, links, inputs, selects, textareas,
  labels, `summary`, `contenteditable`, focusable elements, and the
  ARIA roles button/link/menuitem/tab/checkbox/radio/switch/option) and
  whatever is inside an element marked `data-chain-drag-region="false"`.
  Those keep working normally. Works on elements added later.
- **Double-click** on a drag region does what the OS says the title
  bar does: on macOS, System Settings' "Double-click a window's title
  bar to" (zoom, minimise, or nothing); on Windows and Linux,
  maximise/restore. On macOS it fires on mouse-up and is cancelled if
  the pointer moved, as the real title bar does.
- On macOS a press in a drag region is handled natively (the page
  doesn't receive it), like a real title bar: dragging an inactive
  window works in one go. Hover and clicks on the interactive parts
  still reach the page.
- **`startDrag()`** moves the window with the pointer, for apps with
  their own pointer handling. Call it from a primary-button
  `pointerdown`/`mousedown`; called with no button held, nothing moves.

The SDK finds the drag regions and keeps the native side told about
them as the layout changes, so the page must import `@chain/sdk` (any
import of it) for them to work. Regions are measured in CSS pixels; a
page zoomed with the webview's zoom isn't supported.

## Knowing where the chrome is

`insets()` → `TitleBarInsets`, in CSS pixels from the page's top-left
corner — only chrome that's drawn **over the page** counts:

- `height`: the system title bar's height with `overlay`; 0 with every
  other style (the page starts below the bar, or there is no bar), and
  0 in full screen.
- `windowButtons`: the visible buttons' bounding rectangle, or `null`
  when none are over the page (`standard`/`transparent`, `hidden`, all
  hidden, full screen).
- `left` / `right`: the space from that page edge to the far edge of
  the buttons, for padding a top bar; 0 when there are none on that
  side. macOS buttons are on the left.

`onInsetsChange` fires whenever `insets()` would answer differently:
style or button changes, entering or leaving full screen.

The SDK also keeps these CSS custom properties on the document's root
element up to date (all lengths in `px`):

| Property | Value |
| --- | --- |
| `--chain-title-bar-height` | `height` |
| `--chain-title-bar-inset-left` | `left` |
| `--chain-title-bar-inset-right` | `right` |
| `--chain-window-buttons-x`, `-y`, `-width`, `-height` | `windowButtons`, 0 when `null` |

and the `data-chain-full-screen` attribute on the root element while the
window is in full screen. They're set once the SDK has asked the native
side — a moment after the page starts, not on its very first frame — so
CSS using them should give a fallback.

## Full screen

`isFullScreen()` and `onFullScreenChange(fullScreen)` — no polling.
Entering full screen also fires `onInsetsChange` (macOS hides the
buttons there).

## Platforms

`availability()` never rejects and says which options work here.
Options that don't work are accepted and ignored — never an error.

| Option | macOS | Windows | Linux |
| --- | --- | --- | --- |
| `standard`, `hidden` | yes | yes (`hidden` = no OS frame) | yes (`hidden` = no decorations) |
| `titleBarSize` | yes | ignored | ignored |
| `transparent`, `overlay` | yes | falls back to `standard` | falls back to `standard` |
| `titleVisible` | yes | ignored | ignored |
| `windowButtons` visibility | yes | ignored | ignored |
| `windowButtons.position` | yes | ignored | ignored |
| `appearance` | yes | yes (title bar) | best effort (GTK) |
| `backgroundColor` | yes | yes | yes |
| drag regions, `startDrag` | yes | yes | yes |
| insets | yes | all zero (no chrome over the page) | all zero |
| full screen | yes | yes | yes |
| `showWhen` `firstPaint`, `showCalled` | yes | falls back to `immediately` | falls back to `immediately` |

Only macOS is verified; see `research/`.

## Errors

| Code | When |
| --- | --- |
| `UNSUPPORTED` | outside a Chain app (`availability()` never rejects) |
| `INVALID_ARGUMENT` | `setOptions()` with an unknown key or style, malformed colour, or negative/non-finite position, or with `showWhen`/`showTimeout` |
| `NATIVE_FAILURE` | anything else |

## Non-goals

- Toolbars, title bar accessory views, tabs in the title bar, a
  document proxy icon.
- Window size, position, minimum size, or multiple-window management.
- Vibrancy/translucent materials behind the page.
- Drawing replacement window buttons for `hidden` — the app's job.
- Remembering runtime changes across launches.
- Hiding the window again after its first appearance — `showWhen`
  is about launch only.
- Windows 11 caption-button overlays (`titleBarOverlay`-style) — not
  until a Windows app needs one; `overlay` falls back there.
