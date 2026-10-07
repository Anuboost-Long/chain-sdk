# Browser — Windows research

**Not verified on Windows yet.** The implementation is the same Tauri
code as macOS (wry → WebView2), so it compiles for Windows, but nobody
has run it there. Checklist at the bottom.

## What maps to what

| Need | WebView2 (through wry/Tauri) |
| --- | --- |
| Separate persistent session | `WebviewBuilder::data_directory(<app local data>/browser-sessions/<hash>)` → its own `ICoreWebView2Environment` user-data folder |
| Sign-in popups sharing the session | `on_new_window` → `NewWindowRequested`; `window_features(features)` reuses the opener's environment (`with_environment`) |
| Read the page | `eval_with_callback` → `ExecuteScript`, result already JSON |
| Cookies for fetch | `Webview::cookies()` → `ICoreWebView2CookieManager::GetCookies` |
| Clear an open session | `clear_all_browsing_data()` → `ICoreWebView2Profile2::ClearBrowsingDataAll` (errors on a runtime too old to have it) |
| Clear a closed session | delete the user-data folder |
| Toolbar page | `http://chain-browser.localhost/toolbar` — WebView2 serves custom schemes as `http://<scheme>.localhost` |

## Known risks

- **`cookies()` deadlocks on a synchronous command** (wry#583). Every
  command that reads cookies is `async`.
- **Two environments in one window**: the toolbar uses the default
  environment, the page a session folder. WebView2 allows several
  environments per process as long as each folder is only opened with
  one set of options.
- **Deleting a just-closed session's folder** can fail while
  `msedgewebview2.exe` still holds it for a moment; the clear retries for
  a few seconds, then rejects `UNAVAILABLE`.
- **Back/forward state**: macOS reads `canGoBack`/`canGoForward`; on
  Windows the toolbar keeps both buttons enabled and uses
  `history.back()`/`history.forward()` (WebView2's `CanGoBack` would
  need `webview2-com` in the template crate).
- `*.localhost` hosts are the app's own custom schemes on Windows
  (`http://tauri.localhost` is the app) — the navigation guard blocks
  them, which matters more here than on macOS.

## Verification checklist (whoever has Windows)

1. `availability()` → all true.
2. `open({ url })` beside the app window; toolbar visible; Back/Forward/
   Reload work; the address updates on navigation.
3. Sign in through Microsoft sign-in in a popup; the popup closes and the
   opener is signed in.
4. Quit and relaunch: still signed in (folder under
   `%LOCALAPPDATA%\<identifier>\browser-sessions\`).
5. Two sessions (`session: "a"`, `"b"`) don't share cookies.
6. `read()` returns the page and a same-origin frame; a cross-origin
   frame is listed in `unreadableFrames`.
7. `fetch()` of a login-only image returns its bytes, with the window
   open and closed.
8. `clearSession()` open and closed: the next open shows the login page.
9. An app button press reaches `onButton`; closing the window fires
   `onClose` with `reason: "user"`.
