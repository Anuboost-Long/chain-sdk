# BrowserView Capability (`desktop.browserView`)

## How it works

Other websites shown **inside the app's own window** — a browser tab's
content area, a dev-server preview pane. Requested by Lazify (request
18, `docs/chain-sdk-requests/18-embedded-browser-view.md` in the
lazify-chain repo). macOS 14+ only today.

- **A view is the OS's own `WKWebView`, never a Tauri webview.** Chain
  Core creates it (`browser_view::create`) and adds it to the app
  page's superview, above the page. Two reasons: a Tauri child webview
  in the app's window makes Tauri stop treating that window as a
  "webview window" (`Window::is_webview_window`), which breaks
  `get_webview_window("main")` (dev inspector, attention), drag-drop
  event targeting and every `WebviewWindow` command argument; and a
  Tauri webview would carry the app's IPC bridge and init scripts into
  other sites. A plain `WKWebView` has neither — verified: a page in a
  view sees no `__TAURI_INTERNALS__`, `ipc` or `webkit.messageHandlers.ipc`.
- **The subclass `ChainWebView`** only overrides `willOpenMenu:withEvent:`
  to drop the engine's "… in New Window" and "Download …" menu items
  (dead in a view — WebKit never calls a delegate for them).
- **Placement**: bounds are CSS px in the app page; `place()` scales by
  the page's own `pageZoom`, flips into the page's coordinates if it
  isn't flipped (Tauri's content view isn't), and converts into the
  superview. A `ViewMinYMargin` autoresizing mask keeps the view at its
  distance from the top while the window resizes, until the app calls
  `setBounds()` again.
- **Sessions**: `browser::session_key(name)` → `WKWebsiteDataStore(forIdentifier:)`,
  the same derivation `desktop.browser` uses, so a name is one store
  across both capabilities.
- **One `ViewDelegate` object per view** is its `WKNavigationDelegate`,
  `WKUIDelegate` and KVO observer (`URL`, `title`, `loading`,
  `canGoBack`, `canGoForward` → `onState`, deduplicated). Everything
  lives in a main-thread `thread_local` registry; the registry is never
  borrowed while calling into WebKit (it may call a delegate back
  synchronously).
- **Holding navigations** (`decideNavigation`): `decidePolicyForNavigationAction`
  copies WebKit's `decisionHandler` block into `pending` and emits a
  request; `answer()` (or the Tauri layer's 10 s timer, or `close()`)
  calls it. WebKit asserts if a handler is dropped uncalled, so every
  path calls it. Not asked: the app's own `load/back/forward/reload`
  (`app_navigation` flag, consumed by the next main-frame decision),
  server redirects (WebKit's unpublished `_isRedirect`, checked with
  `respondsToSelector`; fallback: an allowed navigation still
  provisional — a redirect repeats its navigation's own type, so the
  type can't tell), fragment jumps in the same document, and nested
  frames navigating themselves.
- **New windows**: `createWebViewWithConfiguration…` always returns nil
  and emits `onNewWindow` — except a window opened with no address,
  which gets a hidden `WKWebView` (built from WebKit's configuration, as
  required) whose own delegate reports its first real address as the
  opener's `onNewWindow` and cancels it. The Tauri layer drops it after
  10 s (`drop_held`).
- **Input age**: one `NSEvent` local monitor (mouse-down of any button,
  key-down) stamps `last_input` on the view under the click or holding
  first responder. Native events, so a page can't fake them.
- **Load failures**: `didFail(Provisional)Navigation` → `failure_reason(domain, code)`;
  interrupted loads (`NSURLErrorCancelled`, `WebKitErrorFrameLoadInterruptedByPolicyChange`)
  return None and aren't reported. The address is `NSErrorFailingURLKey`.
- **Permissions**: the UI delegate denies media capture and geolocation;
  no other permission has a WebKit delegate hook. Fullscreen is
  `WKPreferences.elementFullscreenEnabled` at creation.
- **Rules and scripts are per session** (`SessionContent` in the
  registry: compiled `WKContentRuleList`s and script sources). Each view
  gets its own `WKUserContentController`; `put_session_content` rewrites
  every view of the session (all rule lists, then `ASK_SCRIPT` in the
  page world, `FRAME_SCRIPT` in Chain's `chain` world, then the app's
  scripts in the page world, all frames, at document start). The engine
  applies changes from each view's next load.
- **Rules** are validated and converted in pure Rust
  (`parse_rules` → `content_rules_json`: Chain's camelCase subset →
  WebKit's kebab-case content-blocker JSON). The compiled list's
  identifier is `chain-view-<session key>-<id hash>-<content hash>`
  (`rule_list_identifier`): `SettingRules` looks it up in
  `WKContentRuleListStore.defaultStore` and compiles only when missing,
  then deletes the same list's other versions by prefix. WebKit's
  compile reason is in the error's `NSHelpAnchor`.
- **The channel**: `chainBrowserView.ask()` (`ASK_SCRIPT`) posts JSON
  text to the page-world `chainBrowserView` handler
  (`WKScriptMessageHandlerWithReply`); the reply block waits in
  `replies` until the app answers (`reply`), 10 s pass (Tauri layer),
  or the view closes. Frame identity is the message's `WKFrameInfo`.
- **Frames for `runInFrames`**: `FRAME_SCRIPT` posts to the `chainFrame`
  handler, which only Chain's world can reach, at every document start;
  a main frame's report resets the list. `run_in_frames` calls
  `callAsyncJavaScript` in the main frame plus each recorded frame, in
  Chain's world, wrapping the body so the result crosses as JSON; a
  `WKErrorJavaScriptInvalidFrameTarget` drops that frame.
- **Picture-in-picture** needs `WKPreferences._setAllowsPictureInPictureMediaPlayback:`
  (unpublished, checked with `respondsToSelector`), set at creation
  only with `allowPictureInPicture`.
- **Lifetime**: the Tauri layer (`packages/cli/templates/browser_view.rs`)
  records each view's owner page and window; a page load start
  (`release_page`, from lib.rs's `release_abandoned_work`) or the
  window's `Destroyed` closes them.

## How to use it

```ts
import { desktop } from "@chain/sdk";

const host = document.querySelector("#tab-content")!;
const id = await desktop.browserView.create({
  url: "https://example.com",
  session: "lazify-web",
  bounds: host.getBoundingClientRect(),
  decideNavigation: true,
  swipeNavigation: true,
  allowFullscreen: true
});
new ResizeObserver(() => desktop.browserView.setBounds(id, host.getBoundingClientRect())).observe(host);

desktop.browserView.onState((state) => state.id === id && renderTab(state));
desktop.browserView.onNavigationRequest((request) => {
  if (request.id !== id) return;
  const backgroundTab = request.modifiers.meta || request.button === "middle";
  if (backgroundTab) openTab(request.url, { background: true });
  desktop.browserView.answer(request.requestId, !backgroundTab);
});
desktop.browserView.onNewWindow((request) => openTab(request.url, { userAction: (request.inputAgeMs ?? Infinity) < 5000 }));

// Switching tabs: keep every view, show one.
await desktop.browserView.setVisible(otherId, false);

// An ad blocker, on the browser's session only.
await desktop.browserView.setRules({ session: "lazify-web", id: "network", rules: networkRules });
await desktop.browserView.setScript({ session: "lazify-web", id: "cosmetics", source: cosmeticsScript });
desktop.browserView.handleMessages(({ frame, message }) => cosmeticsFor(frame.url, message)); // untrusted input

// Media picture-in-picture (view created with allowPictureInPicture: true).
const results = await desktop.browserView.runInFrames(id, `
  if (document.pictureInPictureElement) { await document.exitPictureInPicture(); return "exited"; }
  const video = [...document.querySelectorAll("video")].find((v) => !v.paused);
  if (!video) return "none";
  await video.requestPictureInPicture();
  return "entered";
`);
```

A view always draws above the page: hide it (`setVisible`) while a menu
or dialog must overlap it.

## Files to check

- `capabilities/browser-view/contract.ts` — types.
- `agent-docs/capabilities/browser-view/CONTRACT.md` — semantics, non-goals.
- `crates/core/src/browser_view.rs` — rules (`may_show`, `Bounds`,
  `failure_reason`, initiator mapping) with unit tests; `macos` module:
  view creation, delegates, registry, input monitor, menu pruning;
  `elsewhere`: UNSUPPORTED stubs.
- `packages/cli/templates/browser_view.rs` (and the playground copy) —
  commands, event emission, owner tracking, hold timeouts.
- `packages/cli/templates/lib.rs` — `mod browser_view`, `setup`,
  `on_window_event`, `release_page`, the command list.
- `packages/sdk/src/browser-view.ts` — the SDK wrapper.
- `agent-docs/capabilities/browser-view/research/MACOS.md` — WebKit findings.
