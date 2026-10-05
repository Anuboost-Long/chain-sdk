# Browser — macOS research

What `desktop.browser` stands on, on macOS. Everything here goes through
Tauri 2.12 / wry 0.57's WKWebView, except two small pieces of objc2
(clearing a store, back/forward state). No Swift.

## The window: Tauri multi-webview (`unstable`)

- One `tauri::window::Window` holds two child webviews: the toolbar
  (our own HTML from the `chain-browser` URI scheme) and the page. A
  `WebviewWindow` can only hold one webview, and the toolbar can't live
  inside the school page (we'd have to inject UI into a site we don't
  own) or above it as a frame (LMS pages refuse to be framed).
- `Window::add_child` and `WebviewBuilder` are public only with Tauri's
  `unstable` Cargo feature, so every Chain app's `Cargo.toml` turns it on
  (`patchCargoToml` in `packages/cli/src/scaffold.ts`). It gates API
  visibility only; `tauri-runtime-wry`'s `unstable` feature is empty.
- Child webviews don't follow the window's size, so the layout is
  redone on every `WindowEvent::Resized` (toolbar 44 pt tall, the page
  fills the rest). `set_auto_resize` scales both proportionally, which
  would stretch the toolbar.
- **A Tauri `Window`'s content view spans the whole frame**, title bar
  included (`inner_size() == outer_size()`), so a child at y = 0 sits
  under the title bar. The layout starts below
  `frame.height − (contentLayoutRect.y + contentLayoutRect.height)`
  (`chain_core::browser::title_bar_height`), read on the main thread.
- A native NSToolbar was considered and rejected: it needs an
  Objective-C delegate class here and a separate Win32 toolbar on
  Windows. The HTML toolbar is one implementation for both.

## The session: `WKWebsiteDataStore(forIdentifier:)`

- `WebviewBuilder::data_store_identifier([u8; 16])` → wry creates the
  page's WKWebView with `WKWebsiteDataStore.dataStoreForIdentifier:`
  (**macOS 14+**). Persistent, per app, separate from the app's own
  webview (the default store). Stored under
  `~/Library/WebKit/<bundle id>/WebsiteDataStore/<UUID>/`.
- The 16 bytes are the first 16 of SHA-256(`"chain-browser\0" +
  session`) with UUID version/variant bits set, so a session name maps
  to the same store on every launch without storing anything.
  `dataStoreForIdentifier:` throws for an all-zero UUID; the hash can't
  produce one in practice and the version bits rule it out.
- Below macOS 14 the API doesn't exist. `availability().available` is
  false there and `open()` rejects `UNSUPPORTED` — falling back to the
  default store would share cookies with the app's own webview, which
  the contract promises never happens. (Chain apps' minimum is 12.0.)
- **Clearing**: `dataStoreForIdentifier:` then
  `removeDataOfTypes:allWebsiteDataTypes modifiedSince:distantPast
  completionHandler:`. Works whether or not a webview is using the
  store (unlike `removeDataStoreForIdentifier:`, which fails while any
  WKWebView holds it), and the completion handler tells us when it's
  really gone. Main thread only (objc2 marks the class `MainThreadOnly`),
  so the command dispatches with `run_on_main_thread` and waits on a
  channel from a blocking thread. wry's own `clear_all_browsing_data`
  ignores the completion, so it isn't used.

## Sign-in popups: `on_new_window` + `window_features`

- `window.open` / `target=_blank` arrive at wry's
  `createWebViewWithConfiguration:`. Tauri 2.12's `on_new_window` hands
  us the URL and `NewWindowFeatures`; returning
  `NewWindowResponse::Create { window }` built with
  `WebviewWindowBuilder::window_features(features)` reuses WebKit's
  target `WKWebViewConfiguration` — same data store, and `window.opener`
  / `postMessage` back to the opener work, which Microsoft/Google/Okta
  popups depend on.
- **wry ignores `webViewDidClose:`** (WebView2's `WindowCloseRequested`
  it does handle), so a popup's `window.close()` left it open.
  `close_on_window_close` adds that method to wry's
  `WryWebViewUIDelegate` class at runtime (`class_addMethod`, a no-op if
  it already exists) and closes the NSWindow only when the webview's
  data store has an identifier — i.e. a Chain session, never the app's
  own webview. Verified: the popup closes and returns to the opener.
- Popups get their own `on_new_window` too (nested popups).
- WebKit skips history entries a page added with `pushState` without a
  user gesture when going Back (anti back-button hijacking) — Back from
  such an entry lands on the page before it. Browser behavior, kept.

## Reading the page: `eval_with_callback`

- wry calls `evaluateJavaScript:completionHandler:` and passes the
  result through `NSJSONSerialization` — so the script returns a plain
  object of strings and arrays (anything else would make wry's
  `.unwrap()` on serialization panic). A thrown exception arrives as an
  empty string; the script catches everything itself.
- It runs in the main frame's page world. Frames are read with
  `iframe.contentDocument`, which is `null` for a cross-origin frame —
  exactly the page's own same-origin rule, nothing more.
  `evaluateJavaScript:inFrame:` could reach cross-origin frames; that
  would get around the rule, so it's not used.

## Fetching with the session

- `Webview::cookies()` → `WKHTTPCookieStore getAllCookies:` (HTTP-only
  included). wry's `cookies_for_url` is **not** used: it compares the
  cookie's domain to the URL's host with `==`, so a `.school.edu` cookie
  never matches `canvas.school.edu`, and it ignores paths. Chain Core
  does RFC 6265 domain/path/secure/expiry matching itself
  (`chain_core::browser::cookie_header`).
- NSHTTPCookie's leading dot (domain cookie) is stripped by the `cookie`
  crate, so host-only cookies are matched as domain cookies — they may
  also go to the host's own subdomains. Acceptable: same site, never
  another site.
- Redirects are followed by hand (≤ 10 hops) so each hop gets *its own*
  cookies; reqwest's automatic redirects would drop the Cookie header
  on a host change and couldn't add the next host's.
- The request carries the webview's `navigator.userAgent` (some SSO
  setups bind a session to it).
- When the window is closed, a hidden 1×1 webview on the same store is
  opened just long enough to read the cookies and user agent. Uniform
  with Windows, where the cookie manager only exists on a live webview.

## Safety boundaries verified in Tauri's source

- **Remote pages can't call commands**: `tauri::webview::Webview::on_message`
  ACL-checks every command from a non-local origin, and no capability
  file names a `chain-browser-*` label or a remote URL — so the school
  page can't reach `storage_query` or any plugin.
- **Navigation is limited to http/https/about/data/blob**, and never to
  a `*.localhost` host or the app's dev server: those count as the
  app's own ("local") origin, which *can* call commands.
- **The toolbar's actions check the caller's webview label**: the
  `chain-browser` scheme is registered on every webview, so the page
  could fetch `chain-browser://localhost/action?...` too; the handler
  only acts for the session's own toolbar webview, so the page can't
  fake an app-button press.
- **App events never reach the page**: Tauri only evaluates an event
  into webviews that registered a JS listener for it, and registering
  one (`plugin:event|listen`) is an ACL-checked command the page can't
  call — so page URLs, titles and button ids only reach the app.
- The app-wide `on_page_load` hook (storage rollback, recording cancel)
  ignores browser webviews — a school page loading must not cancel the
  app's recording.

## Verified on

macOS 27.0.1, Apple Silicon — see AGENTS.md's Status.
