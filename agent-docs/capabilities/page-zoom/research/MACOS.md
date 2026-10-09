# PageZoom — macOS Research

Verified on macOS 27 (Retina display), 9 October 2026.

Tauri's `Webview::set_zoom(f64)` calls wry, which sets WKWebView's
`pageZoom` (macOS 11+). Measured in a throwaway `chain init` app:

| factor | `innerWidth` (CSS px) | `devicePixelRatio` |
| ------ | --------------------- | ------------------ |
| 1      | 800                   | 2                  |
| 1.5    | 533                   | 3                  |
| 0.75   | 1066                  | 1.5                |
| 1.25   | 640                   | 2.5                |

So it's real reflow, and the backing scale goes up with it, which keeps
canvas content sharp. After `location.reload()` the page came back at
1.25 (640 px, ratio 2.5). WebKit keeps `pageZoom` on the webview, and
the factor Chain stores per webview label stays in step.

There's no zoom getter through Tauri, so `page_zoom_get` returns the
factor last passed to `set`. Anything else that changes `pageZoom` (only
Tauri's `zoomHotkeysEnabled` shortcuts, which Chain apps don't turn on)
would make it stale.

`JSON` turns `NaN` into `null`, which fails native deserialization, so
the SDK rejects non-finite numbers itself.

Not verified: xterm.js rendering at 75% and 150% by eye. The pixel-ratio
numbers say it should be sharp, and Lazify will look.
