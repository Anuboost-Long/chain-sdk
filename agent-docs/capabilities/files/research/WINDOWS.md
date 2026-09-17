# Files — Windows Research

Source: initial survey in `docs/chain-sdk-requests/02-files.md` (mneme's
capability request). **Not yet verified on real Windows hardware/CI — no
Windows machine was available while implementing this. Per chain-sdk
rule 3 (no single-platform contracts), this capability's contract stays
Draft, not stable, until someone runs the checklist below.**

## Approach

Same as macOS — `std::fs` is the same standard-library API on Windows;
there is no Windows-specific Rust code in `crates/core/src/files.rs`.

- **Data directory**: `app_handle.path().app_data_dir()` resolves to
  `%APPDATA%\<AppName>\` — handled by Tauri, same as `storage`. `files`
  creates a `files\` subdirectory there.

## Known risks to verify (not yet confirmed either way)

- **`MAX_PATH` (~260 char) limits.** This is the specific risk the
  request document flagged, and it's the reason `write()` never accepts
  a caller-provided filename: every stored file gets a short,
  capability-generated id (`generate_id()` — 16 hex chars) plus at most a
  short extension, never the app's own descriptive filename. The
  `attachment` table's `file_name` (display) vs. `file_path` (this
  capability's opaque reference) split already assumed this split, per
  the request doc. Still worth confirming the full resolved path
  (`%APPDATA%\<AppName>\files\<16-hex-chars>.<ext>`) stays comfortably
  under 260 chars for realistic `<AppName>` lengths — it should, by a
  wide margin, but "should" isn't "confirmed."
- **Antivirus/Defender transient file locks.** Smaller risk here than for
  `storage` (one write per file, not SQLite's constant read/write churn),
  but a `write()`/`delete()` racing a real-time scan is still worth
  triggering deliberately at least once.
- **Path separator assumptions.** `is_valid_reference()` in
  `crates/core/src/files.rs` rejects any reference containing anything
  other than ASCII alphanumerics and `.` — this already excludes both
  `/` and `\`, so there's no Windows-specific separator handling needed
  in the validator itself. Worth double-checking `PathBuf::join` behaves
  as expected with these Windows-clean reference strings (it should,
  since they never contain a separator of either kind).

## Checklist for whoever verifies this on Windows

- [ ] `cargo build`/`cargo check`/`cargo test -p chain-core` succeed.
- [ ] `chain doctor` + a fresh `chain init` + `npm run dev` produces a
      working app whose `files_write`/`files_read`/`files_resolve_path`/
      `files_delete` commands work end to end (mirror the macOS
      verification steps in `AGENTS.md`).
- [ ] Confirm the full resolved path length in a realistic `%APPDATA%`
      location stays well under `MAX_PATH`.
- [ ] Trigger a write/delete while Defender real-time protection is
      active and confirm no spurious lock failures under normal use.
- [ ] Update `docs/CAPABILITY_MATRIX.md`'s Windows column and this
      capability's `component.json` once confirmed.
