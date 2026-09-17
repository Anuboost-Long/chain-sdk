# Storage — macOS Research

Source: initial survey in `docs/chain-sdk-requests/01-local-storage.md`
(mneme's capability request), expanded during implementation.

## Approach

SQLite is a portable C library — there is no separate macOS "database"
service to bridge to, unlike Clipboard/Audio-style capabilities. The same
Rust implementation (`crates/core/src/storage.rs`, using `rusqlite` with
the `bundled` feature) covers macOS and Windows identically. What's
platform-specific is narrow:

- **Build-time only**: Xcode Command Line Tools (`clang`) to compile the
  vendored SQLite C source. Already a baseline requirement for building
  any Tauri app on macOS (`chain doctor` territory) — nothing new.
- **Data directory**: resolved via Tauri's `app_handle.path().app_data_dir()`,
  which already resolves correctly to
  `~/Library/Application Support/<bundle-id>/` on macOS — nothing to
  hand-roll.
- **App Sandbox**: no entitlement needed unless the app is ever
  distributed through the Mac App Store. Not a concern for `chain dev`/
  `chain build`'s default (unsandboxed) `.dmg` distribution. Revisit only
  if/when Mac App Store distribution becomes a real requirement.

## Verified

- `cargo build`/`cargo check` succeed with `rusqlite = { features =
["bundled"] }` on this machine (macOS aarch64) — the vendored SQLite C
  source compiles cleanly with the Xcode Command Line Tools already
  installed for Tauri itself.
- Migrations, parametrized query, and parametrized execute were run for
  real against a file-backed database in `mneme` (see
  `capabilities/storage/AGENTS.md` for the exact verification steps) —
  not just compiled.

## Not yet verified

- Behavior under macOS's own file-locking/App Nap edge cases during
  long-running writes. Not hit in initial testing; revisit if it comes
  up.
