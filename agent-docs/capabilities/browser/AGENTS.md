# Browser Capability — Agent Memory

Scope: `desktop.browser` — a separate signed-in browser window the app
controls. Requested by mneme (request 36,
`docs/chain-sdk-requests/36-signed-in-browser-window.md` in the mneme
repo) for importing LMS pages behind single sign-on.

## What's already decided

- **Tauri multi-webview, not native toolbars**: one HTML toolbar for
  macOS and Windows. Costs the `unstable` Cargo feature in every app.
- **No fallback store below macOS 14**: unavailable there, so the
  separation from the app's webview always holds.
- **The session is identified by name only** (hashed); nothing about it
  is stored by Chain. One window per session.
- **Credentials never cross into JS**: cookies stay in Rust for
  `fetch()`; no API returns them; the app can't run script in the page.
  `READ_SCRIPT` and `navigator.userAgent` are the only scripts Chain runs
  there; back/forward use `history.*` only on Windows.
- **Reading respects same-origin**: frames through `contentDocument`
  only. Never `evaluateJavaScript:inFrame:` / per-frame WebView2 scripts.
- **Cookie matching and redirects are ours** (wry's `cookies_for_url`
  is too strict on domains, reqwest's redirects drop/can't add cookies).
- **Toolbar trust is by webview label** in the scheme handler.
- Don't grow this into tabs, downloads, an editable address bar, form
  filling, cookie access or POST fetches without a real requirement
  (CONTRACT.md's non-goals).

## Status

- [x] macOS 27.0.1 (Apple Silicon), `apps/playground` dev build driven
      through the dev inspector, against a local two-origin test site,
      2026-10-04:
      - availability → all true; errors: invalid URL/session/duplicate
        buttons → `INVALID_ARGUMENT`, `read()` closed → `NOT_FOUND`.
      - open with buttons (one disabled); toolbar under the title bar
        (found and fixed: Tauri's content view spans the frame).
      - sign-in cookie set during a redirect; still signed in after a
        relaunch; session "b" isolated; opens beside the app window
        when there's room, centered otherwise; fixed title kept.
      - `read()`: top page, same-origin frame and its nested frame,
        cross-origin frame in `unreadableFrames`.
      - `fetch()`: HttpOnly cookie through a redirect, 404 →
        `HTTP_ERROR` with response, `maxBytes` → `TOO_LARGE`; with the
        window closed too (hidden webview, gone afterwards).
      - `window.open` popup: requested size, `postMessage` to the opener,
        closes itself (found and fixed: wry ignored `webViewDidClose:`).
      - history-API navigation → `onNavigate` with the new URL/title;
        duplicate/empty-title events removed.
      - toolbar button `.click()` in the toolbar webview → `onButton`
        with url/title; disabled button → nothing; a forged
        `chain-browser://…/action` fetch from the page → refused.
      - the page's `invoke("storage_query")` → refused by the ACL;
        navigating the page to the dev server → blocked.
      - `clearSession()` closed and open (reloads signed out); `close()`
        → `onClose` `reason: "app"`; second `close()` a no-op.
- [x] Propagated to mneme with `chain update` (Cargo.toml, lib.rs,
      browser.rs; second run "Already up to date"); mneme's native
      project type-checks with dev and release features; `tsc` clean.

## Not done yet

- [ ] A physical mouse click on the toolbar and `onClose` with
      `reason: "user"` (no Accessibility permission for synthetic
      clicks during verification; the code path is the same Destroyed
      handler).
- [ ] mneme's own window exercising it (mneme builds its side next).
- [ ] A real LMS with Microsoft/Google sign-in and multi-factor.
- [ ] Windows — everything; see `research/WINDOWS.md`'s checklist.
- [ ] Contract tests under `capabilities/browser/tests/`.
