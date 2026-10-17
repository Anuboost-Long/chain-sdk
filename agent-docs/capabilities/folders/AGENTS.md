# Folders Capability — Agent Memory

Scope: `desktop.folders` — real paths inside folders the user granted
(picked or dropped, read-write, persisted) plus folders the app declares
read-only in `package.json`, with list, read, atomic write, move, delete
and watch. Requested by Lazify (requests 01, 02, 06, 10 and 14 in
`lazify-chain/docs/chain-sdk-requests/`), and it also answers Mneme's
request 11 (`external-file-read`).

Read order for a task in this capability:

1. Root `/AGENTS.md`
2. `/docs/ARCHITECTURE.md`
3. This file
4. `CONTRACT.md` + `capabilities/folders/contract.ts`
5. `research/MACOS.md` / `research/WINDOWS.md`
6. `crates/core/src/folders.rs`, `packages/cli/templates/folders.rs`,
   `packages/sdk/src/folders.ts`

## What's already decided

- **A separate capability, not `files`.** `files` keeps its
  no-real-paths rule (Lazify asked explicitly that it not be weakened).
- **One grant model for both requests' design fork:** picked/dropped
  grants are read-write, declared folders are read-only. Declared folders
  come from `package.json` `"chain.readOnlyFolders"`, validated by
  `packages/cli/src/readOnlyFolders.ts` and compiled in as
  `CHAIN_READ_ONLY_FOLDERS` (set by `tauriEnv()`, read with
  `option_env!` in `templates/folders.rs`). It's always set, even to
  `[]`, so Cargo rebuilds when the list changes. No runtime API can add a
  grant without the user: an allow-list the running app could extend
  would be bookkeeping, not a boundary.
- **Request 06 (the app's own folders) is `folders.appFolder()`**: two
  always-present read-write roots (`Folders::with_app_folders`), not a
  new capability, since they need exactly the same operations and path
  rules. `data` is `<app data>/app`, a sibling of `folder-grants.json`,
  and `with_app_folders` refuses a data folder that would contain the
  grants file. They're left out of `grants()` so "reopen the user's
  folders" code doesn't pick them up. There's no run/instance id (the
  request left that to Lazify).
- **Request 02 (watching) is `folders.watch()`**, not its own capability.
  It needs the same grants and the same path rules.
- **Every path is resolved and checked natively on every call**
  (`Folders::resolve`): absolute, no `..`, longest existing part
  canonicalised, inside a grant giving the needed access. Symlinks are
  followed only into grants; `stat`/`move`/`delete` act on the link
  itself. Declared roots are re-resolved per call so one can appear while
  the app runs.
- **Errors use a new `ChainErrorCode`, `NOT_GRANTED`,** distinct from
  `PERMISSION_DENIED` (the OS refused, e.g. TCC). An app reacts to those
  differently: ask the user to pick the folder, or send them to System
  Settings.
- **`readText` is strict UTF-8.** Lossy decoding would let a
  read-modify-write of `.env` corrupt it silently.
- **Grants persist in `<app data>/folder-grants.json`**, written
  atomically. A corrupt file is logged and treated as empty rather than
  stopping the app.
- **Process `cwd` checks go through `Folders::working_directory`** —
  `process-runner` and `terminal` use it from `templates/lib.rs` and
  `templates/terminal.rs`, so their core modules don't depend on this
  one.
- **File work runs off the main thread** (`off_main` in
  `templates/folders.rs`). Sync Tauri commands run on the main thread,
  and a recursive listing would freeze the window.
- **Request 14 (copy across volumes) is a `move()` fallback, not a
  `copy()`.** The requester said it would answer the whole request, and
  it needs no new method or error code. On `EXDEV` (`ERROR_NOT_SAME_DEVICE`
  on Windows, both `ErrorKind::CrossesDevices`), `move_across_volumes`
  copies into `.chain-move-<id>/` beside the target, renames into place,
  then removes the source. The walk is plain `std::fs` (`fs::copy` is
  `fcopyfile` with `COPYFILE_ALL` on macOS, so modes, ACLs and xattrs
  come along), so the same code serves Windows.
- **Bytes travel raw:** `readBytes` returns `tauri::ipc::Response`, and
  `writeBytes` sends one raw body (u32 LE path length, path, bytes).

## Status

**Implemented and verified end to end on macOS**, 9 October 2026. Not
on Windows (rule 3), so it stays `draft`/`experimental`.

- `crates/core/src/folders.rs`: 14 unit tests (`cargo test -p chain-core
  folders`). They cover persistence and dedupe, outside/relative/`..`
  rejection, symlinks into and out of grants, case and `/var`
  differences, declared read-only folders with a byte-range read, a
  missing declared folder being `NOT_FOUND`, atomic writes keeping `0600`,
  strict UTF-8 and `maxBytes`, recursive listing with skips and a symlink
  loop, moves (collision, case-only rename, root), create/delete
  idempotence, `working_directory`, and a real FSEvents change.
- End to end in a throwaway `chain init` app under `chain dev` (driven
  with `chain inspect`), with `chain.readOnlyFolders` set in its
  `package.json`:
  - `grants()` listed the declared folder as `readOnly`/`declared`.
  - In it: recursive `list`, `stat`, a `readBytes` range (`"456"`) and
    `readText` all worked. `maxBytes` gave `TOO_LARGE`, a write
    `NOT_GRANTED`, `/etc/hosts` `NOT_GRANTED`, a relative path
    `INVALID_ARGUMENT`, and watching a missing folder `NOT_FOUND`.
  - A recursive `watch` reported `created` then `modified` for a file
    written from outside the app.
  - `pick()` in the real sheet granted the chosen folder, canonical and
    persisted to `folder-grants.json`, and `revoke()` emptied the file.
  - In a read-write grant: an atomic `.env` rewrite, `createFolder`
    (nested), a 300 KB `writeBytes` round trip (byte-exact), a case-only
    rename, `move` onto an existing file (`INVALID_ARGUMENT`), deleting
    the root (`INVALID_ARGUMENT`), delete `toTrash`, and a recursive
    delete. That grant was made by a probe-only command added to the
    throwaway app (the picker kept opening on another folder); the file
    operations themselves went through the real SDK and IPC.
- Listing speed: 52,903 entries in ~170 ms (release), against ~265 ms
  for Node. See `research/MACOS.md`.

### App folders (Lazify request 06) — verified on macOS, 9 October 2026

- Unit test `app_folders_are_read_write_grants_but_not_listed`: read-write
  under `data`, usable as a working directory, the grants file unreachable,
  the root undeletable, a cleared `temp` recreated, and a data folder that
  would contain the grants file refused.
- End to end in the throwaway app: `data` came back as
  `~/Library/Application Support/dev.chain.lazyprobe/app` and `temp` as
  `/private/var/folders/…/T/dev.chain.lazyprobe`. A shadow repo ran
  through `process-runner`: `git --git-dir=<data>/shadow-repos/… --work-tree=<project>`
  `init`/`add -A`/`status` showed `A  .env`, after `info/exclude` was
  written through `writeText`, and the project got no `.git`. `ls` ran
  with `cwd` set to `temp`, and the folder-grants file was rejected
  (`..` → `INVALID_ARGUMENT`, direct → `NOT_GRANTED`).
- In Lazify itself (reported by its session after `chain update`): its
  shadow repos now live under `appFolder("data")/shadow-repos/<run id>`.
  For a plain project in `appFolder("temp")`, the agent changes panel
  showed an edit and a new file correctly (Modified +2/−1, Untracked +1,
  right diff), and the project got no `.git`.

### Declared files (Lazify request 10) — 9 October 2026

- A declared entry can be a single file. Access was already exact: a root
  matches by whole path components, so a file root matches only itself.
  The change is that `grants()` and the roots now take the kind from disk
  (`kind_on_disk`), so a declared file reports `kind: "file"`, and a
  missing one `"folder"` until it exists.
- Unit test `a_declared_file_grants_that_file_alone_read_only` covers
  `NOT_FOUND` before the file exists, then a read, and `kind: "file"`.
  A sibling file and the parent folder are `NOT_GRANTED`, a write is
  `NOT_GRANTED`, and listing the file is `INVALID_ARGUMENT`. Lazify
  checks its usage panel against it in its own app.

### Moving across volumes (Lazify request 14) — verified on macOS, 11 October 2026

- Unit test `a_move_across_volumes_copies_links_and_modes_and_leaves_nothing_on_failure`
  calls `move_across_volumes` on one volume: a `0755` file, a relative
  `.bin` symlink, a dangling symlink and a `0700` folder come through,
  a `0555` folder can still be filled, and an unreadable file fails the
  copy with the source intact and no target or `.chain-move-` folder left.
- A real cross-volume move through `move_entry`, with a temporary probe
  test (since removed) and an APFS disk image mounted on another device:
  `apps/playground/node_modules` (80 MB, 735 entries) moved in ~160 ms.
  `diff -r --no-dereference` against a reference copy was clean. All 7
  symlinks and 6 executables survived, `.bin/tsc --version` ran from
  the new volume, and the source was gone. Moving onto an existing
  folder gave `INVALID_ARGUMENT`. On a 30 MB image the copy ran out of
  space (`ENOSPC`, `NATIVE_FAILURE`): the volume was left empty and the
  source byte-identical.
- No SDK or template change: the `folders_move` command and
  `desktop.folders.move()` are unchanged, so apps get it on their next
  build of `chain-core`.
- Lazify adopted it (lazify-chain `e7688d7`): one `move()` places a new
  project, and on the "copied, original not removed" error it keeps `to`
  and deletes the staging folder. Not yet run across volumes from Lazify
  itself.

## What's NOT done yet

- [ ] Drag-and-drop granting (`onDrop`) end to end — needs a real drag.
- [ ] A real TCC denial surfacing as `PERMISSION_DENIED`.
- [ ] Windows: everything in `research/WINDOWS.md`'s checklist; `toTrash`
      there (`IFileOperation`).
- [ ] Contract tests under `capabilities/folders/tests/`.
- [ ] Tell Mneme that request 11 is answered by declared folders +
      `readBytes({ offset })` (no separate capability) — it's Mneme's
      call to adopt it.
- [ ] Windows: a cross-volume `move` of an `npm install`ed tree. Symlinks
      there need Developer Mode or a privilege; `node_modules/.bin` holds
      `.cmd` shims, but a tree with real links would fail the copy without it.
- [ ] A cross-volume `move` to a network share (SMB) on macOS.

## Rules specific to this capability

- Never add a way for the running app to grant itself a path. Grants
  come from the user (picker, drop) or the build (`readOnlyFolders`).
- Never weaken `desktop.files`' no-real-paths rule to share code with
  this one.
- Every new method must go through `Folders::resolve` with the right
  access and follow flag. Never touch a caller's path before it's
  resolved.
- No ignore-file handling, globbing, or format knowledge (`.env`, JSON)
  here — the app owns all of that.
