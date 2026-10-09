# Folders — macOS Research

Verified on macOS (Darwin 27, APFS) while implementing
`crates/core/src/folders.rs`, 9 October 2026.

## No native adapter

Everything is `std::fs` plus two crates: `dunce` (canonical paths; a
no-op on macOS, it only strips `\\?\` on Windows) and `notify` 8
(FSEvents). The picker is `rfd`, already used by `files.pick()`. The
Trash is `NSFileManager.trashItemAtURL` through `objc2-foundation`. No
Swift.

## Canonical paths, case and `/var`

libc `realpath(3)` (what `std::fs::canonicalize` calls) on macOS both
resolves symlinks (`/var` → `/private/var`, `/tmp` → `/private/tmp`) and
**returns each existing component in its on-disk case** on a
case-insensitive APFS volume. Checked: `realpath FOO` for a folder `Foo`
prints `…/Foo`. (Python's `os.path.realpath` does *not* correct case;
don't use it to reason about this.)

So the grant check is a plain component-wise `starts_with` on canonical
paths, with no case folding: the root was canonicalised when it was
granted, and the requested path's longest existing prefix is
canonicalised per call. A tail that doesn't exist yet (a file about to be
written) is appended as given, under an already canonical parent.

Gotcha found by a failing test: `stat()` canonicalises only the parent
(it must not follow a final symlink), so a case-different final name came
back in the caller's case. It now canonicalises the full path when the
entry isn't a symlink. `resolve()` itself must not do that, or a
case-only `move()` (`Readme.md` → `README.md`) would resolve the target
to the source and rename onto itself.

## Case-only renames

`rename("Readme.md", "README.md")` works on APFS. The "target already
exists" check sees the source at the target path, so `move()` compares
`(dev, ino)` and allows it when they're the same file.

## Privacy protection (TCC)

An unsandboxed app reading `~/Desktop`, `~/Documents`, `~/Downloads` or
iCloud Drive for the first time triggers a system consent prompt. A
denial surfaces as `EPERM` ("Operation not permitted"), which
`io::ErrorKind::PermissionDenied` maps to `PERMISSION_DENIED`, never
`NOT_FOUND`. `canonicalize` hits the same `EPERM`, and `real_path()` only
walks up past `NotFound`, so a TCC denial isn't mistaken for a missing
path. A picked folder in those locations doesn't need the prompt: the
open panel grants access to what the user chose. Not separately verified
with a real denial.

## Atomic writes

Temp file `.<name>.<id>.chain-tmp` in the same folder (same volume, so
`rename(2)` is atomic), `write_all`, `sync_all`, copy the old file's
permissions, then `rename`. Verified: a `0600` `.env` stays `0600`, and
no `.chain-tmp` is left behind. Writing through a symlink resolves to the
target first, so the link stays a link.

## Listing performance

A recursive `list()` of `~/Work/lazify` (52,903 entries, `node_modules`
included, nothing skipped) took ~170 ms in a release build, single
threaded, against ~265 ms for the equivalent Node walk (`readdir`
`withFileTypes` + `lstat` per entry, all in parallel promises). JSON for
that listing is ~9 MB. `DirEntry::metadata()` is an `lstat` per entry;
the file type alone would be free (`d_type`), but size and mtime need it.
No parallelism was needed.

## FSEvents through `notify`

`notify::recommended_watcher` is FSEvents on macOS. Recursive by nature;
`NonRecursive` filters in `notify`. Writing a new file under a watched
folder reported `created` then `modified` for its canonical path within
about a second. `Access` events are dropped (reads aren't changes).
`need_rescan()` (FSEvents' `MustScanSubDirs`, dropped events) becomes a
`rescan` change for the watched root.

## Picker

`rfd::AsyncFileDialog::pick_folder(s)`, plus `pick_file_or_folder` /
`pick_files_or_folders` for `files: true`, which rfd only has on macOS.
It's a sheet on the app window, and `files::open_panel_guard` keeps it
and `files.pick()`/`save()` from overlapping. The panel opens on the last
folder it showed: during verification, clicking Open straight away
granted that folder (another repo) instead of the intended fixture. The
grant was correct for what was clicked, but apps should show the granted
path back to the user.

## Drops

Tauri delivers `WindowEvent::DragDrop(Drop { paths })` with real paths
when `dragDropEnabled` is on (the default). Granting happens natively
only while the page has an `onDrop()` subscriber (`folders_accept_drops`),
and the flag is cleared when the page reloads (`release_page` in the
page-load hook). **Not verified end to end**: it needs a real drag.

## Not verified

- A real TCC denial (`PERMISSION_DENIED`).
- Drag-and-drop granting.
- `rescan` under real overflow.
