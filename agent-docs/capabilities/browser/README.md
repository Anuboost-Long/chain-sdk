# Browser Capability (`desktop.browser`)

## How it works

A separate OS window that's a real browser for one site the user signs
in to, with a session of its own. Requested by mneme (request 36,
`docs/chain-sdk-requests/36-signed-in-browser-window.md` in the mneme
repo) to import LMS pages behind single sign-on without ever holding a
password.

- **The window** is a Tauri `Window` with two child webviews (Tauri's
  `unstable` feature, which every Chain app's `Cargo.toml` turns on): a
  44 pt toolbar on top and the page below it. The toolbar is our own
  HTML (`crates/core/src/browser_toolbar.html`), served by the
  `chain-browser` URI scheme. It talks back by fetching `/state` and
  `/action?do=…`, and the scheme handler only answers the session's own
  toolbar webview, so the page can't fake a button press. Rust updates
  it with `eval("chainToolbar.update(…)")`.
- **A session** is a name → 16 bytes (SHA-256, `session_key`). On macOS
  those bytes are the `WKWebsiteDataStore(forIdentifier:)` UUID (macOS
  14+); on Windows they name a WebView2 user-data folder under the app's
  local data dir. Persistent, separate from the app's webview and from
  each other. One window per session; window labels are
  `chain-browser-<hex>` (+ `-page`, `-toolbar`, `-popup-N`, `-cookies`).
- **Sign-in popups**: `on_new_window` builds a window from WebKit's /
  WebView2's own features (same store, `window.opener` intact). On macOS
  a runtime-added `webViewDidClose:` closes a popup that calls
  `window.close()`.
- **Navigation guard**: the page may only go to http/https/about/data/
  blob, never `*.localhost` or the dev server (the app's own origin,
  which may call commands). Remote pages can't call commands anyway
  (Tauri's ACL), and app events never reach them.
- **Reading** runs `READ_SCRIPT` through `eval_with_callback` in the
  main frame: `outerHTML`, then every frame whose `contentDocument` the
  page itself can reach (same origin), recursively; the rest are listed
  as unreadable.
- **Fetch with the session** reads the store's cookies (the open page's
  webview, or a hidden 1×1 one on the same store), matches them per
  RFC 6265 in Chain Core (`cookie_header`), and follows redirects itself
  so every hop gets its own cookies, with the webview's user agent.
- **Sign out** clears the store: on macOS `removeDataOfTypes:` on the
  identified store (works whether or not it's in use); on Windows
  `ClearBrowsingDataAll` on an open page or deleting the folder.
- The app-wide `on_page_load` hook (storage rollback, recording cancel)
  ignores `chain-browser-*` webviews.

## How to use it

```ts
import { desktop } from "@chain/sdk";

const { available } = await desktop.browser.availability();

const stopButtons = desktop.browser.onButton(async ({ id, url, title }) => {
  if (id === "done") return desktop.browser.close();
  const page = await desktop.browser.read(); // { url, title, html, frames, unreadableFrames }
  const picture = await desktop.browser.fetch(new URL("/logo.png", page.url).href);
  // picture.bytes, picture.contentType
});
desktop.browser.onNavigate(({ url, title }) => {});
desktop.browser.onClose(({ reason }) => {}); // "user" | "app"

await desktop.browser.open({
  url: "https://canvas.school.edu",
  buttons: [
    { id: "import", label: "Import this page" },
    { id: "done", label: "Done" }
  ]
});
await desktop.browser.setButtons([{ id: "import", label: "Import this page", enabled: false }, { id: "done", label: "Done" }]);
await desktop.browser.clearSession(); // Sign out
```

Every method takes `{ session }` (default `"default"`) to keep two sites
apart. Full semantics, defaults and errors: `CONTRACT.md`.

## Files to check

- `capabilities/browser/contract.ts` — the TypeScript contract.
- `packages/sdk/src/browser.ts` — SDK wrapper: error prefixes,
  `HTTP_ERROR` for fetch, synchronous unsubscribe.
- `packages/cli/templates/browser.rs` (mirrored by hand in
  `apps/playground/src-tauri/src/browser.rs`) — the window, webviews,
  popups, toolbar scheme handler and commands. Registered in
  `templates/lib.rs` (`.manage`, `.register_uri_scheme_protocol`,
  `generate_handler!`); tracked by `chain update` like `dev_inspector.rs`.
- `crates/core/src/browser.rs` — session keys, navigation guard, read
  script and result, cookie matching, session fetch, beside placement,
  and macOS objc2 bits (store support/clearing, history, title bar,
  `webViewDidClose:`). `cargo test -p chain-core browser`.
- `crates/core/src/browser_toolbar.html` — the toolbar.
- `packages/cli/src/scaffold.ts` — `patchCargoToml` adds `"unstable"`.
- `research/MACOS.md`, `research/WINDOWS.md` — platform findings and
  the Windows checklist.

Debugging: the window not laying out right → `layout()` /
`title_bar_height`; a toolbar action doing nothing → the scheme handler's
label check; `read()` → `UNAVAILABLE` right after open → wry drops
callbacks of scripts queued before the first load finishes.
