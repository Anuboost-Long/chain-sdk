# BrowserView Capability — Agent Memory

Scope: `desktop.browserView` — other sites embedded in the app's own
window. Requested by Lazify (request 18,
`docs/chain-sdk-requests/18-embedded-browser-view.md` in the lazify-chain
repo) for its browser tabs and agent dev-server preview.

## What's already decided

- **Native `WKWebView`, not a Tauri child webview** (README "How it
  works" has why: Tauri's webview-window detection, and IPC isolation).
  Don't "simplify" this back to `window.add_child`.
- **Events, never blocking calls**, for navigation decisions — WebKit
  holds the decision block; WebView2 has a Deferral. 10 s, then refused.
- **No disposition, no sticky activation**: raw `modifiers`/`button` and
  a native `inputAgeMs`; the app derives intent (WebKit has neither).
- **Normalized `reason` for load failures**; engine codes only in
  `nativeCode`/`nativeDomain`.
- **Permissions fixed at creation**: fullscreen toggle; camera/mic/
  geolocation always denied; nothing per-site.
- **Same session store as `desktop.browser`** for the same name.
- **No script execution in views yet** — reaching nested/cross-origin
  frames needs frame discovery WebKit has no public API for. It's the
  next request (media PiP, Lazy Shield cosmetic), not a quick add-on.
- **Menu**: remove dead "New Window"/"Download" items rather than wire
  them up (the menu doesn't give the link's address publicly).
- Unpublished WebKit used, each behind `respondsToSelector`: `_isRedirect`
  (redirects not re-asked; fallback heuristic otherwise), and
  `WKPreferences._setAllowsPictureInPictureMediaPlayback:` (only with
  `allowPictureInPicture`; the user chose opt-in over leaving PiP out,
  17 Oct 2026 — no public macOS switch exists).
- **Rules are Chain's camelCase subset of the content-blocker format**,
  not Chrome's DNR: DNR has no element hiding, and its initiator-domain
  and priority semantics don't map onto WebKit. Lazify converts.
- **Scripts run in the page's world, the channel is untrusted.** Only
  the frame identity (from WebKit) can be relied on.
- **`runInFrames` runs in Chain's own world**, frames discovered by
  `FRAME_SCRIPT` reporting in (WebKit can't list frames publicly).
- **No blocked counter**: only unpublished API reports blocks.

## Status

- [x] macOS 27 (Apple Silicon), `apps/playground` dev build driven
      through the dev inspector plus a temporary native probe
      (`evaluateJavaScript`, the view's page-process pid, its frame —
      reverted), against a local two-origin test site, 16–17 Oct 2026:
      - placement: requested bounds match the WebKit frame after the
        flip, at page zoom 1 and 1.5; `setBounds`, `setVisible`; views
        close on app page reload (`NOT_FOUND` after).
      - state: load start/finish, title, back/forward, `pushState`,
        fragment jumps; `url` changes when a load starts and returns to
        the shown page after a failure.
      - `decideNavigation`: link held → deny stays / allow goes; 10 s
        timeout refuses; app `load/back` not asked; server redirect
        asked once (bug found and fixed: redirects repeat the original
        type); nested same-origin frame → top asked with
        `fromMainFrame: false`; frame navigating itself not asked; close
        with a held request, no WebKit assertion.
      - new windows: `target=_blank`, `window.open` with features
        (`width`/`height`), open-then-address-later reported, no-address
        window dropped silently after 10 s.
      - failures: host-not-found (-1003), unreachable (-1004), blocked
        (restricted port, WebKit 103), insecure (expired, self-signed);
        `stop()` not reported. Bug found and fixed: failing address is
        `NSErrorFailingURLKey`.
      - crash: `kill -9` of the view's own WebContent pid → `onCrash`;
        `reload()` restarts it (bug found and fixed: WebKit forgets the
        URL, so the last shown one is kept).
      - sessions: cookie + localStorage shared on one name, absent on
        another; mic/camera → `NotAllowedError`, geolocation → code 1,
        no prompt, no TCC activity (only once the window is visible —
        WebKit defers while hidden); `fullscreenEnabled` true only with
        `allowFullscreen`; app-origin link refused; no Tauri/IPC
        globals in the page; `INVALID_ARGUMENT`/`NOT_FOUND` errors.
      - real input (a person clicking): `button: "primary"`, Cmd-click
        → `meta: true`, `inputAgeMs` ≈ 110 ms; script navigations
        `button: null`; context menu: Back → held `kind: "history"`,
        menu `reload:` → held `kind: "reload"`; link menu after pruning
        shows only Open Link / Copy Link (before: Open Link in New
        Window and Download Linked File did nothing).
- [ ] Not verified: `swipeNavigation` with a real trackpad swipe,
      element fullscreen entering/leaving, `inspectable`'s Inspect
      Element item, `middle`/`secondary` buttons (mask values 4/2 are
      from WebKit's source, not observed), file-upload panel, use in
      Lazify's own window (Lazify is porting its browser and preview
      onto it and will report these).
- [x] Rules and scripts (lazify request 21), same setup, 17 Oct 2026:
      - rules: an image blocked in the page and a cross-origin frame
        (the server never saw the requests); `.ad` hidden; another
        session untouched; a replaced list applies from the next load;
        a localhost `ifDomain` exception lifts rules for everything the
        page loads; `removeRules` restores; every resource and load
        type compiles; a blocked page document → `onLoadFailed`
        `blocked` (WebKit 104, mapping added); 150,001 rules refused.
      - errors: unsupported action → `INVALID_ARGUMENT` `rule 1: …` (bug
        fixed: Tauri's own argument parsing said NATIVE_FAILURE); `|`/
        `{n}` named by index; engine refusals carry its reason from
        `NSHelpAnchor` (bug fixed: the generic "error 6").
      - cache: 60k pattern-heavy rules compile in ≈10 s; the same rules
        again ≈1 s, also after relaunching the app; one rule changed
        recompiles. Crash found and fixed: WebKit's
        `removeContentRuleListForIdentifier:` calls a nil completion
        handler.
      - scripts: run in every frame before the page's scripts, in its
        world; `ask()` round-trips from each frame with the right
        `frame` (`mainFrame` false for nested); works under
        `script-src 'none'`; handler throws / non-JSON answer / no
        handler / 10 s timeout all reject in the page; async handler
        answers; `removeScript` stops it; closing mid-ask, no assertion.
      - `runInFrames`: page plus three nested frames (two cross-origin);
        a throw in one frame → that frame's `error`; unknown view →
        `NOT_FOUND`; a removed iframe errors once then is dropped (bug
        fixed: pruned by the error code, not its message).
      - picture-in-picture: from a cross-origin frame through
        `runInFrames`, enters and exits; `navigator.userActivation.isActive`
        true; the same call from a timer 1.5 s later → `NotAllowedError`
        (the gesture lapses). Off by default → `NotSupportedError`, also
        for a canvas-stream video.
- [x] Request 21 propagated to Lazify with `chain update`, 17 Oct 2026
      (browser_view.rs, lib.rs); its native `cargo check` with chain
      dev's features clean. Lazify builds media PiP first, then Lazy
      Shield (separate network/cosmetic lists).
- [x] Lazify's field report (tickets 042/043, macOS 27, `chain dev`):
      media PiP from a cross-origin iframe through `runInFrames`; Lazy
      Shield with Ghostery's full lists as one 120,492-rule list (a
      listed image never reached the server; off/on through
      removeRules/setRules); cosmetics over `ask()` (18 KB of styles).
      Old compiled versions are deleted: one ContentRuleList file left
      after several on/off cycles. Numbers: fresh compile ≈25 s (50 MB
      compiled); re-enable after removeRules ≈13 s; unchanged content
      ≈7.4 s. Chain's own share of that last one, benchmarked on 120k
      rules / 25.7 MB: ≈2.6 s in a debug build (JSON parse 0.6, checks
      0.3, conversion 0.8, SHA-256 0.85), ≈0.2 s in release; the rest is
      the IPC hand-over and WebKit loading the 50 MB list (not timed).
      Per-rule `unlessTopUrl` on ~115k rules tripled the compiled size
      and compile time (CONTRACT.md now warns).
- [ ] Possible, not done: optimizing `sha2`/`serde_json` in apps' dev
      profile (most of the debug-build cost above), or hashing less.
      Lazify declined it (17 Oct 2026): only dev builds are slow, users
      run release. Don't do it without a new need.
- [x] Propagated to Lazify with `chain update`, 17 Oct 2026 (added
      browser_view.rs, merged lib.rs); Lazify's native `cargo check`
      with chain dev's features clean. The first attempt half-applied
      because the CLI's compiled `dist/` predated the new
      `TRACKED_FILES` entry: rebuild `packages/cli` after changing
      scaffold.ts.
- [ ] Windows: not started (`elsewhere` stubs, UNSUPPORTED). The stub
      module hasn't even been compiled for Windows — this Mac lacks the
      Windows SDK headers the C dependencies need. WebView2 plan:
      `CoreWebView2Controller` bounds, `NavigationStarting`/
      `NewWindowRequested` with `GetDeferral()`, `ProcessFailed`,
      `PermissionRequested` → deny.
- [ ] Known gaps to raise when a real need appears: the default WebKit
      user agent (no `Safari/` token — some sites treat it as an
      embedded browser); `alert/confirm/prompt` dismissed; downloads.
