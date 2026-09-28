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

## `pick()` — the open panel as a window-attached sheet (request 16)

Source: mneme's `docs/chain-sdk-requests/16-native-file-picker-sheet.md`.

**The problem is in wry, not the web layer.** An `<input type="file">`
in the webview reaches wry's WKUIDelegate
(`wry-0.57.0/src/wkwebview/class/wry_web_view_ui_delegate.rs:100-124`,
unchanged in 0.55.1), which builds an `NSOpenPanel` and calls
`runModal()`. That presents a free-floating app-modal window (no sheet
animation) and spins a nested run loop on the main thread until it
closes. Safari/Chrome use `beginSheetModalForWindow:completionHandler:`
instead — the sheet macOS animates out of the window's title bar. No
web-side option changes which one wry calls.

**Chosen: `rfd` 0.17's `AsyncFileDialog` with `set_parent(window)`.**
Verified by reading `rfd-0.17.2/src/backend/macos/modal_future.rs`, not
assumed: `ModalFuture::new` takes the parent `NSWindow` (derived from the
raw `AppKitWindowHandle` via `window_from_raw_window_handle`), and when
`NSApplication.isRunning` and a window exists, it `run_on_main`s a block
that calls the panel's `begin_modal` → `beginSheetModalForWindow_completionHandler`
(`file_dialog/panel_ffi.rs:72`). The completion handler fills shared
state and wakes the Rust future — the main thread is never blocked. The
`runModal()` path is only the fallback when the app isn't running or no
window exists (it prints a "fallback to sync dialog" line to stderr).

- `set_parent` takes anything implementing raw-window-handle 0.6's
  `HasWindowHandle + HasDisplayHandle`, which Tauri's `Window` does — so
  `crates/core` depends on `raw-window-handle`'s traits, never on Tauri
  (the Tauri command in `templates/lib.rs` passes its `tauri::Window` in).
- Extension filters map to `NSOpenPanel.allowedContentTypes` inside rfd.
- `rfd` already depends on the same `objc2`/`objc2-app-kit` family Tauri
  2 uses, so it adds little on macOS. Its default features only pull
  Linux portal/Wayland crates on Linux targets.
- Bytes are read after the sheet closes with `std::fs::read` — the
  picked path never leaves Rust (the `files` contract's no-paths rule).
- Returning bytes: a JSON `Vec<u8>` (what `files_read` does) turns 20 MB
  into ~70 MB of JSON text. `files_pick` returns a raw
  `tauri::ipc::Response` instead — a small JSON header followed by the
  files' bytes back to back, which JS receives as an `ArrayBuffer`.

### Verified

See `AGENTS.md`'s Status section for the end-to-end run (sheet
attached to the window, cancel → `[]`, a real pick → correct name/size/
bytes).
