# Attention — macOS Research

Investigated on macOS 27.0.1 (Darwin 27), 9 October 2026.

## Notifications need an app bundle

`UNUserNotificationCenter.current()` in a bare binary raises an uncaught
`NSInternalInconsistencyException`: `bundleProxyForCurrentProcess is nil:
mainBundle.bundleURL file:///…`. That aborts the whole process, and an
`Info.plist` embedded in the binary (`-sectcreate __TEXT __info_plist`,
as Tauri does for dev builds) doesn't help: the bundle identifier is
there, but LaunchServices has no bundle proxy. `chain dev` runs exactly
such a binary, so `swift/ChainAttention.swift` checks
`Bundle.main.bundleURL.pathExtension == "app"` before every call and
reports `unavailable` otherwise. `cargo test` exercises that path (its
test binary is unbundled and doesn't abort).

`NSUserNotificationCenter` (deprecated since macOS 11) still returns a
center in a bare binary. Not used: it's deprecated, and nobody looked at
whether it displays anything on macOS 27.

## Unresolved: macOS refused every ad-hoc test bundle here

Every attempt to get permission on this machine failed with
`UNErrorDomain` code 1, "Notifications are not allowed for this
application", without a prompt:

- the `chain build` output in a scratch folder under `/private/tmp`
  (only the executable is linker-signed, with an identifier that doesn't
  match the bundle id);
- the same bundle after `codesign --force --deep -s -` (identifier then
  `dev.chain.lazyprobe`, still ad-hoc);
- a minimal hand-made `Mini.app` (ad-hoc signed) in `/private/tmp`.

Run from `~/Applications`, `Mini.app` waited instead, and the user saw
and allowed a prompt. But the completion handler never fired, and a
relaunch got code 1 again. The notification daemon's log wasn't readable
from the terminal, and neither was `~/Library/Preferences/com.apple.ncprefs.plist`
(it's protected). Lazify's own Electron build (`com.lazify.desktop`) is
also ad-hoc signed, so ad-hoc signing alone isn't a proven cause.

Candidates to check: Gatekeeper/quarantine or App Translocation rules for
unnotarized apps, LaunchServices registration of apps outside
`/Applications`, and a Developer ID signature. **The successful path
(`shown`, then a click) is unverified on any machine so far.** What is
verified: the refusal comes back as `{ outcome: "failed", message }`
through the real IPC, not as a crash and not as a false `denied`.

## Focus and the Dock bounce

Both are Tauri's own window methods: `Window::is_focused()`,
`WindowEvent::Focused`, and
`request_user_attention(Some(UserAttentionType::Informational))` (which
is `NSApp.requestUserAttention(.informationalRequest)`, one bounce, only
while the app is inactive). Verified under `chain dev`: `isFocused()`
went `true` → `false` after `osascript` activated Finder, and
`onFocusChange` fired with `false`. The bounce call succeeds. That it
visibly bounced was not observed.

## Click → bring the window back

`UNUserNotificationCenterDelegate` is installed at launch
(`attention::setup` in the template), so a click that activates the app
is delivered too. The click handler un-minimises, shows and focuses the
window that called `notify()` (remembered by id), then emits
`chain://attention-click` to that webview. Unverified because of the
above.
