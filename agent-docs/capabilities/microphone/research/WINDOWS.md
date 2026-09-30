# Microphone — Windows research (unverified)

Nobody has run this on Windows yet.

- wry 0.55.1's WebView2 `PermissionRequested` handler (`src/webview2/mod.rs`)
  only auto-allows `CLIPBOARD_READ`. Microphone requests keep WebView2's
  default state, which shows WebView2's own permission prompt (it
  remembers the answer per origin). That meets "never auto-deny" but adds
  a second prompt on top of the OS privacy setting. If that's unwanted,
  Chain can handle `PermissionRequested` for `MICROPHONE` itself through
  Tauri's `with_webview` — decide once it's actually been seen.
- Windows privacy (Settings → Privacy & security → Microphone → "Let
  desktop apps access your microphone") is the OS gate; nothing is
  declared at build time.
- Expected `MediaRecorder` output: `audio/webm;codecs=opus`.

Checklist: prompt behavior on first `getUserMedia`, behavior with the
desktop-apps toggle off (`NotAllowedError`?), and the default mimeType.
