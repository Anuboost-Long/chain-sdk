# Window — Windows and Linux research

**Not verified on Windows or Linux.** The code paths are Tauri's own
cross-platform window calls (no native adapter), so they compile from
the same template, but nobody has run them there. Checklist at the
bottom.

## What maps to what (Tauri 2.11)

| Option | Windows | Linux (GTK) |
| --- | --- | --- |
| `standard` | default frame | default decorations |
| `hidden` | `set_decorations(false)` — no caption, no caption buttons; Tauri keeps the resize border and shadow | `set_decorations(false)` |
| `transparent`, `overlay` | **falls back to `standard`.** Tauri's `TitleBarStyle` is macOS-only. A real overlay needs `WM_NCCALCSIZE` handling plus drawing or hosting caption buttons (Windows 11 snap layouts hang off the maximize button's `HTMAXBUTTON` hit-test) — not worth building before a Windows app asks | falls back; client-side decorations are the GTK equivalent, unbuilt |
| `titleVisible` | ignored — no API hides only the caption text (`SetWindowText("")` also blanks the taskbar entry) | ignored |
| button visibility | ignored — Win32 can only disable minimize/maximize (`WS_MINIMIZEBOX`/`WS_MAXIMIZEBOX`), not hide them one by one | ignored |
| button position | ignored — the system owns caption button placement | ignored |
| `appearance` | `set_theme` → `DwmSetWindowAttribute(DWMWA_USE_IMMERSIVE_DARK_MODE)` for the caption | `set_theme` → GTK prefer-dark, theme-dependent |
| `backgroundColor` | `set_background_color` on the window and its WebView2 (`DefaultBackgroundColor`; alpha other than 0 becomes 255) | window + WebKitGTK background |
| drag regions / `startDrag` | `start_dragging` → `ReleaseCapture` + `WM_NCLBUTTONDOWN(HTCAPTION)` | `gtk_window_begin_move_drag` |
| double-click | toggles maximize (the caption's own behaviour) | toggles maximize |
| insets | all zero: no style draws chrome over the page | all zero |
| full screen | `is_fullscreen` and `Resized` events | same |

`availability()` reports exactly this (portable logic in
`chain_core::window::availability`, `TitleBarStyle::supported`).

## `showWhen` (request 40)

Not done: `firstPaint` and `showCalled` fall back to `immediately` and
`availability().showWhen` says so. To add it: Windows has no alpha-0
equivalent through Tauri (it needs `WS_EX_LAYERED` +
`SetLayeredWindowAttributes`), and WebView2 in a hidden window may stop
producing frames (Chromium occlusion tracking), so `hide()`/`show()`
alone would likely still show an unpainted frame. Check both before
turning the flags on. Linux (WebKitGTK) has the same open question.

## Known risks

- `set_decorations(false)` at startup happens in setup, after Tauri
  created the window, so on Windows the first frame may still show the
  caption for a moment. If that's visible, the fix is setting
  `decorations: false` in the generated window config instead.
- WebView2 can't take a translucent background: a colour with alpha
  becomes opaque.

## Verification checklist (whoever has Windows)

1. No `chain.window` options → the window is unchanged.
2. `availability()` → `transparent`/`overlay`/`titleVisible`/
   `windowButtons*`/`titleBarInsets` false, the rest true.
3. `"titleBarStyle": "hidden"` → no caption; window still resizes.
4. `setOptions({ titleBarStyle: "overlay" })` → no error, `options()`
   reads back `standard`.
5. `appearance: "dark"` → dark caption; back to `"system"` follows the OS.
6. `backgroundColor` → no white flash on launch.
7. A `data-chain-drag-region` element drags; a button inside it clicks;
   double-click maximizes and restores.
8. F11/maximize to full screen → `onFullScreenChange(true)`.
