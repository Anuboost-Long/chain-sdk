# Storage — Windows Research

Source: initial survey in `docs/chain-sdk-requests/01-local-storage.md`
(mneme's capability request). **Not yet verified on real Windows
hardware/CI — no Windows machine was available while implementing this.
Per chain-sdk rule 3 (no single-platform contracts), this capability's
contract stays Draft, not stable, until someone runs the checklist
below.**

## Approach

Same as macOS — `rusqlite` with the `bundled` feature compiles the same
vendored SQLite C source for both platforms; there is no Windows-specific
Rust code in `crates/core/src/storage.rs`.

- **Build-time only**: MSVC Build Tools (the C++ workload) are needed to
  compile the vendored SQLite source — already a baseline requirement
  for building any Tauri app on Windows, nothing new for this
  capability specifically.
- **Data directory**: `app_handle.path().app_data_dir()` resolves to
  `%APPDATA%\<AppName>\` on Windows — handled by Tauri, nothing to
  hand-roll.

## Known risk to verify (not yet confirmed either way)

- **WAL mode over network-mapped drives**: SQLite's WAL journal mode
  (which `Database::open` enables — see `crates/core/src/storage.rs`)
  has documented issues on network filesystems (SMB/mapped drives) on
  Windows — locking can misbehave or WAL can fail to checkpoint. Since
  `app_data_dir()` normally resolves to a local user profile path, this
  likely doesn't affect the common case, but it should be checked
  explicitly rather than assumed safe, especially for roaming-profile
  or managed-desktop environments where `%APPDATA%` can be redirected to
  a network share.
- **Antivirus/Defender file locking**: real-time protection can
  transiently lock a database file during writes, which can surface as
  a spurious `NATIVE_FAILURE` from `desktop.storage.execute()`. Worth
  confirming whether `rusqlite`'s busy-timeout handling
  (not yet configured explicitly — see AGENTS.md TODO) is sufficient, or
  whether the capability needs its own retry-on-busy logic.

## Checklist for whoever verifies this on Windows

- [ ] `cargo build`/`cargo check` succeed with the `bundled` SQLite
      feature using MSVC Build Tools.
- [ ] `chain doctor` + a fresh `chain init` + `npm run dev` produces a
      working app whose `storage_open`/`storage_migrate`/`storage_query`/
      `storage_execute` commands work end to end (mirror the macOS
      verification steps in `capabilities/storage/AGENTS.md`).
- [ ] Confirm `%APPDATA%` resolution is a local path in the target
      environment, or explicitly test WAL behavior if it isn't.
- [ ] Trigger a write while Defender real-time protection is active and
      confirm no spurious lock failures under normal use.
- [ ] Update `docs/CAPABILITY_MATRIX.md`'s Windows column and this
      capability's `component.json` once confirmed.
