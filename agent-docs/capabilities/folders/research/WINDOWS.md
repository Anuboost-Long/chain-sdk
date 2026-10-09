# Folders — Windows Research

**Nothing here has been compiled or run on Windows.** This is what the
code does on Windows and what needs checking. Lazify ships a Windows
build today, so this capability is on its Windows critical path.

## Paths

- `dunce::canonicalize` returns `C:\…` instead of `std`'s `\\?\C:\…`
  where that's lossless, so paths shown to the app and passed to Git look
  normal. Paths over `MAX_PATH` (260) keep the `\\?\` form, which `std::fs`
  accepts. Deep `node_modules` trees exceed 260 routinely.
- `GetFinalPathNameByHandle` (behind canonicalize) returns the on-disk
  case and an upper-case drive letter, so the `starts_with` grant check
  needs no case folding, as on macOS. To verify: `c:\project\SRC\a.ts`
  under a grant for `C:\Project`.
- `absolute()` rejects `..`, and `Path::is_absolute` requires a drive or
  UNC prefix (`\foo` isn't absolute on Windows).
- `same_entry()` (case-only renames) falls back to comparing lower-cased
  paths, since `MetadataExt::file_index` is unstable.

## Locked files

Editors, antivirus and dev servers hold files open, so writes, renames
and deletes fail with `ERROR_SHARING_VIOLATION` (32) or
`ERROR_LOCK_VIOLATION` (33). Those map to `UNAVAILABLE`. The atomic
write's final `rename` (std uses `MoveFileExW` with
`MOVEFILE_REPLACE_EXISTING`) retries 5 times with 50–250 ms backoff.
`ReplaceFileW` would keep the target's ACLs and alternate streams;
consider it if permissions matter on Windows.

## Trash

`toTrash` returns `UNSUPPORTED`. `IFileOperation` with
`FOFX_RECYCLEONDELETE` is the API to use.

## Picker

rfd's `pick_folder(s)` is `IFileOpenDialog` with `FOS_PICKFOLDERS`. Files
and folders in one dialog isn't possible there, so `pick({ files: true })`
rejects `UNSUPPORTED`.

## Watching

`notify` uses `ReadDirectoryChangesW`. Its buffer overflows under heavy
churn (`git checkout`, `npm install`), and `notify` reports that as an
error or rescan, which becomes a `rescan` change.

## Checklist

- [ ] Grant `C:\Users\me\src\app`, list it recursively with
      `skipFolders: ["node_modules"]`, time it against Node.
- [ ] Read and write a file whose path is over 260 characters.
- [ ] Atomic `writeText` while VS Code has the file open.
- [ ] `move` a folder whose files a dev server holds → `UNAVAILABLE`.
- [ ] Case-only rename.
- [ ] `watch` + `npm install` in the folder → `rescan` or changes, no crash.
