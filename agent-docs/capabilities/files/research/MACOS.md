# Files — macOS Research

Source: initial survey in `docs/chain-sdk-requests/02-files.md` (mneme's
capability request), expanded during implementation.

## Approach

Reading/writing/deleting bytes inside the app's own managed directory is
`std::fs` — a portable standard-library API, not a native macOS surface.
There is no macOS-specific Rust code in `crates/core/src/files.rs`, the
same reasoning `storage` already established for `rusqlite`.

- **Data directory**: `app_handle.path().app_data_dir()` — the exact same
  call `storage` already uses — resolves to
  `~/Library/Application Support/<bundle-id>/` on macOS. The `files`
  capability creates a `files/` subdirectory there, a sibling of
  `storage`'s `app.db`; nothing new to hand-roll.
- **Serving a stored file to the webview**: Tauri's asset protocol
  (`convertFileSrc`, `@tauri-apps/api/core`) is already part of Tauri
  core on every platform. No new Tauri plugin, no new
  `capabilities/default.json` permission entry — confirmed by checking
  how `storage` reaches its own custom Tauri commands: app-defined
  `#[tauri::command]`s registered via `tauri::generate_handler!` in the
  app's own binary aren't gated by the plugin permission/ACL system the
  way a *plugin's* commands are (`storage_migrate`/`query`/`execute`
  aren't listed in `apps/playground/src-tauri/capabilities/default.json`
  either, and they work) — the original request's assumption that this
  would need its own capability/permission entry doesn't hold here.
- **App Sandbox**: same as `storage` — no entitlement needed unless the
  app is ever distributed through the Mac App Store.

## Verified

- `cargo test -p chain-core` passes `files::tests::write_read_delete_round_trip`
  and `files::tests::rejects_path_traversal_references` on this machine
  (macOS aarch64).
- `cargo check` succeeds for both `crates/core` and
  `apps/playground/src-tauri` with the new module wired in.

## Not yet verified

- Real end-to-end exercise through a running app's webview (write from
  JS → read back → render via `url()`'s `convertFileSrc` path) — see
  `AGENTS.md`'s Status section for what's actually been run for real vs.
  just compiled/unit-tested at the time this doc was last touched.
- Behavior under macOS's own file-locking edge cases during a write that
  races a delete of the same reference. Not hit in initial testing;
  revisit if it comes up (the id-based naming makes two writes never
  collide, but a concurrent read/delete of the same reference from two
  calls isn't explicitly locked beyond what the OS gives for free).
