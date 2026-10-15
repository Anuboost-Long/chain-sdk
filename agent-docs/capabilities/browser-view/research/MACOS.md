# BrowserView — macOS Research

Request: `lazify-chain/docs/chain-sdk-requests/18-embedded-browser-view.md`.
Findings below are from reading the vendored `wry` 0.55.1 and
`objc2-web-kit` 0.3.2 source (`~/.cargo/registry/src/.../wry-0.55.1`,
`.../objc2-web-kit-0.3.2`) and, where noted, from a live runtime probe
against this machine's actual WebKit framework (`protocol_copyMethodDescriptionList`
on `WKUIDelegate`/`WKNavigationDelegate` — see "What's genuinely verified"
at the bottom). No implementation exists yet; this is pre-contract
research per `docs/CAPABILITY_WORKFLOW.md` step 2.

## What wry's safe API already covers

- **Embedding and bounds**: `WebviewBuilder::with_bounds(Rect)` — a child
  webview positioned/sized relative to its parent. Exactly the
  embedding primitive this request needs; no raw platform code required
  for this part.
- **Named persistent sessions**: `WebViewBuilderExtUnix`/macOS equivalent
  `with_data_store_identifier([u8; 16])` — a `WKWebsiteDataStore`
  identified by UUID, isolated from the app's own webview and from every
  other identifier. `desktop.browser` (`crates/core/src/browser.rs`)
  already uses this exact mechanism for its own named sessions — this
  request's "two named sessions" need nothing new here, just the same
  derivation (session name → stable UUID) applied per guest instead of
  per browser window.
- **Load state**: `with_on_page_load_handler(Fn(PageLoadEvent, String))`
  — `Started`/`Finished`, with the URL. Covers "a load starting and
  finishing" directly.
- **Title**: `with_document_title_changed_handler(Fn(String))`. Covers
  "the page's title changing" directly.
- **Crash**: `with_on_web_content_process_terminate_handler(Fn())` —
  "Available on macOS and iOS only" per its own doc comment. Covers
  "the guest's own content process having crashed or gone away," but
  carries no detail (no error, no code) — matches the request (it only
  asks that a crash be reported as its own event).

None of the above need raw `objc2` access; they go through wry's public
builder.

## What wry's safe API does not cover — needs direct WKUIDelegate/WKNavigationDelegate access

`desktop.browser` already reaches past wry into the raw `PlatformWebview`
handle to swizzle one WKUIDelegate method (`webViewDidClose:`,
`crates/core/src/browser.rs:382`) because wry has no handler for it. This
request needs the same technique for several more methods, because wry's
convenience handlers don't carry enough information:

- **`with_navigation_handler(Fn(String) -> bool)`** only hands over the
  destination URL — no source-frame URL, no main-frame flag, no
  modifier/button info. The request needs all of those, so this needs
  `WKNavigationDelegate`'s real
  `webView:decidePolicyForNavigationAction:decisionHandler:`, whose
  `WKNavigationAction` parameter has `sourceFrame`/`targetFrame`
  (`WKFrameInfo`, each with `isMainFrame`), `request` (the destination),
  `modifierFlags`, and `buttonNumber`
  (`objc2-web-kit/.../generated/WKNavigationAction.rs`).
- **`with_new_window_req_handler(Fn(String, NewWindowFeatures) -> NewWindowResponse)`**
  — `NewWindowFeatures` has `size`/`position`/`opener`, nothing else.
  The real `WKUIDelegate` method,
  `webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:`,
  hands over the same rich `WKNavigationAction` as above (so the
  initiating frame/main-frame/modifier/button signals are available
  here too) plus `WKWindowFeatures`
  (`menuBarVisibility`/`statusBarVisibility`/`toolbarsVisibility`/
  `allowsResizing`/`x`/`y`/`width`/`height` — all `NSNumber?`, i.e. each
  individually "unspecified" or a value).
- **Failed loads with a real error code**: no wry handler exists for
  this at all. Needs `WKNavigationDelegate`'s
  `webView:didFailProvisionalNavigation:withError:` (before a page
  loads) and `webView:didFailNavigation:withError:` (after it started
  rendering) — both hand an `NSError` (domain, code, description) and,
  via the method's own context, which navigation (hence which frame)
  failed.

## Two things the Electron/Chromium model has that WebKit's public API does not

These are real gaps against what the request describes Electron doing,
not implementation choices — WebKit's documented, versioned public API
(confirmed by the generated bindings, not guessed) has no equivalent:

- **No "disposition" (`foreground-tab`/`background-tab`/`new-window`).**
  Chromium computes this internally and Electron surfaces it on
  `did-create-window`; nothing in `WKNavigationAction` or
  `WKWindowFeatures` names it, and no WebKit delegate method returns
  anything like it. What *is* available, from `WKNavigationAction`, is
  `modifierFlags` + `buttonNumber` for the action that triggered the new
  window, plus `WKWindowFeatures`' visibility/resizing/size hints — the
  same raw signals a browser would derive its own disposition-like
  classification from. Lazify's own policy code would need to compute
  its equivalent from these, the same way its Electron code lets
  Chromium do that computation for it today. This is worth raising with
  Lazify before locking the contract's shape for new-window requests,
  so the field name doesn't promise something WebKit can't give.
- **No "has the guest seen genuine user input (not pointer movement) in
  the last few seconds" signal.** Chromium's internal "sticky user
  activation" isn't exposed through any WebKit delegate method either —
  `WKNavigationAction` only carries the modifier/button state of the
  specific action being decided, not a standing recency flag. Native
  has no API to ask "was there a click in this guest recently" as a
  yes/no. The only way to give the app this signal is for Chain to
  deliver guest-level real-input events (pointer-down, key-down —
  excluding pointer movement) as they happen, timestamped, and let the
  app compute its own recency threshold — which is also squarely "the
  policy decision stays app code" per the request's own framing. This
  plumbing likely shares a lot with the two-finger swipe-gesture
  requirement, which also needs raw input observed inside a guest.

## Permission policy: the request's five permission names map unevenly onto real WKUIDelegate methods

A live runtime probe of this machine's actual WebKit framework
(`protocol_copyMethodDescriptionList` on the loaded `WKUIDelegate`/
`WKNavigationDelegate` Objective-C protocols — not just what
`objc2-web-kit` happens to bind) found **exactly nine** optional
`WKUIDelegate` methods:

```
webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:
webView:requestDeviceOrientationAndMotionPermissionForOrigin:initiatedByFrame:decisionHandler:
webView:requestGeolocationPermissionForOrigin:initiatedByFrame:decisionHandler:
webView:requestMediaCapturePermissionForOrigin:initiatedByFrame:type:decisionHandler:
webView:runJavaScriptAlertPanelWithMessage:initiatedByFrame:completionHandler:
webView:runJavaScriptConfirmPanelWithMessage:initiatedByFrame:completionHandler:
webView:runJavaScriptTextInputPanelWithPrompt:defaultText:initiatedByFrame:completionHandler:
webView:runOpenPanelWithParameters:initiatedByFrame:completionHandler:
webViewDidClose:
```

(`WKNavigationDelegate` has 16, listed in full below, all of them
navigation-lifecycle — none permission-shaped.)

So **geolocation, camera/microphone and device-orientation-and-motion**
are real per-request, per-origin, decide-synchronously-or-later asks —
these map cleanly onto the request's "ask per guest and per permission
name, answer allow or deny" shape, including the one-off and
synchronous-check cases (the `decisionHandler` block can be invoked
immediately for a cached decision, or later for a real ask — same
mechanism either way).

**Fullscreen, pointer lock, keyboard lock, DRM (`mediaKeySystem`), and
clipboard write have no delegate method at all in this WebKit** — they
were never a gap in `objc2-web-kit`'s bindings; the underlying
Objective-C protocol itself has no such method on this machine's WebKit
to bind. WebKit's real model for these, as far as the public API goes:

- **Fullscreen**: gated wholesale by
  `WKWebViewConfiguration.preferences.elementFullscreenEnabled` — a
  boolean set once when the guest's webview is configured, not a
  per-request decision. There is no "ask, then answer allow/deny" for
  an individual fullscreen request to intercept.
- **Pointer lock / keyboard lock**: not verified to exist in WKWebView's
  public API at all on this OS version — no delegate method surfaced,
  and nothing else in `objc2-web-kit`'s `WKPreferences` bindings names
  them either. Needs a dedicated check (does `document.requestPointerLock()`
  even resolve inside a `WKWebView`?) before promising anything here.
- **DRM / EME**: also preference-gated wholesale
  (`WKPreferences.isEncryptedMediaAPIEnabled` et al. in WebKit's own
  docs), not a per-origin ask.
- **Clipboard write**: WebKit allows a programmatic clipboard write
  during a real user gesture without any permission delegate call at
  all (the web platform's own gesture-gated model, not an
  ask-the-app-first one) — there is nothing to intercept.

**This means the request's permission list can't be implemented
uniformly on macOS.** Three of the five (geolocation, camera/mic,
device-motion aren't even in the request — only mentioned as an
example) are real per-ask intercepts; the five the request actually
names (fullscreen, pointer lock, keyboard lock, DRM, clipboard write)
are *config toggles decided once at guest creation* at best, and
possibly entirely ungated. Raised with Lazify (see the reply this
research produced) rather than silently promising a decide-per-request
hook that WebKit has no method to back.

## What's genuinely verified vs. not

- **Verified live, on this machine, today (14 October 2026)**: the
  exact `WKUIDelegate`/`WKNavigationDelegate` method lists above, via a
  small Swift probe using `protocol_copyMethodDescriptionList` against
  the real, loaded WebKit framework — not Apple's documentation, not
  `objc2-web-kit`'s binding coverage (which could lag the real
  framework). This is the strongest evidence available short of writing
  the real guest and testing against live sites.
- **Not verified**: whether `with_bounds`, `with_data_store_identifier`,
  and the swizzling technique `browser.rs` already uses actually compose
  correctly for a *child* webview inside a window that also hosts the
  app's own primary webview (today's `browser.rs` swizzle targets a
  window's only webview). No guest has been created yet. Whether
  blocking any Rust thread inside a `decidePolicyForNavigationAction`
  callback (the alternative to the async-decisionHandler design below)
  would deadlock against JS has not been tested empirically — this
  research resolves the design question a different way (see
  `CONTRACT.md`'s reasoning once written) rather than testing the
  deadlock directly, so that risk is avoided rather than measured.
- **Not started**: Windows (WebView2's `CoreWebView2Controller` for
  embedding/bounds, `NavigationStarting`/`NewWindowRequested` events with
  a `Deferral` object for the async-decision shape, and its own
  permission model) — the request itself says Windows parity isn't
  blocking (Lazify's first release is macOS-only, D-2).

## Permission policy, revised after Lazify's reply — no per-request hook needed

Lazify's actual policy (`src/main/browser/browser-permissions.ts`) turns
out to be a **static allow-list, never a per-site decision**: fullscreen/
pointer-lock/keyboard-lock/DRM/clipboard-sanitized-write are always
allowed, everything else is always denied, with no site ever getting a
different answer and no site ever being asked. Confirmed with them on
15 October 2026, after sharing the findings above.

That changes what this capability needs to offer for permissions
entirely: **not a per-request decide event, just a fixed set of
capability toggles set once when a guest is created** —
`WKWebViewConfiguration.preferences.elementFullscreenEnabled`,
`isEncryptedMediaAPIEnabled`, and camera/mic/geolocation left at their
default (denied, since `WKUIDelegate`'s real per-request methods for
those simply go unimplemented when the app doesn't swizzle them — an
unhandled `WKUIDelegate` method means WebKit denies on its own, no
native code needed to refuse it). No `decidePolicyFor…`-shaped async
event is needed for permissions at all; that pattern (see above) is only
needed for navigation/new-window.

Pointer lock and keyboard lock stay unconfirmed to even work in
`WKWebView` (see above) — this no longer blocks anything, since an
off-by-default toggle degrades safely either way: if the API doesn't
exist, there's nothing to turn on; if it does and defaults to off,
behavior matches "always denied" for those two until Lazify's "always
allowed" expectation for them is checked against a real build.

If a future caller needs real per-site permission decisions (allow this
origin's camera, deny that one's), that's a reason to add a
`decidePolicyFor…`-shaped event for permissions later — not something
to build speculatively now (rule 7).

## Full `WKNavigationDelegate` method list (for completeness)

```
webView:didFailProvisionalNavigation:withError:
webView:didFinishNavigation:
webView:didFailNavigation:withError:
webViewWebContentProcessDidTerminate:
webView:decidePolicyForNavigationAction:decisionHandler:
webView:didStartProvisionalNavigation:
webView:decidePolicyForNavigationAction:preferences:decisionHandler:
webView:authenticationChallenge:shouldAllowDeprecatedTLS:
webView:decidePolicyForNavigationResponse:decisionHandler:
webView:didCommitNavigation:
webView:didReceiveAuthenticationChallenge:completionHandler:
webView:didReceiveServerRedirectForProvisionalNavigation:
webView:navigationAction:didBecomeDownload:
webView:navigationResponse:didBecomeDownload:
webView:shouldGoToBackForwardListItem:willUseInstantBack:completionHandler:
webView:willSubmitForm:submissionHandler:
```
