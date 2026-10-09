# Folders Capability — Contract

## What this is

Work with real files in folders the user chose. The user grants a folder
(or a file) through the OS picker or by dropping it onto the window. From
then on the app can list, read, write, move, delete and watch anything
under it, using real absolute paths, until the grant is revoked. Grants
survive restarts.

Driven by Lazify's request 01 (a developer workbench whose whole subject
is the user's project folders), request 02 (change events in those
folders), request 06 (folders of the app's own), and Mneme's request 11
(read-only access to other tools' data, such as `~/.claude/projects`).

This is deliberately **not** `desktop.files`. `files` stores the app's own
blobs behind opaque references and never shows a real path. That rule
stays. A repository can't be an opaque reference: Lazify shows its paths,
passes them to Git, and opens them in an editor.

## The design fork, resolved: picked grants plus declared read-only folders

Lazify and Mneme both need to read folders the user never picks:
`~/.claude/projects`, `~/.codex/sessions`, an old app profile. One grant
model serves both requests:

- **Picked or dropped grants** are read-write. The user chose them, in
  the OS's own picker or by dragging them onto the window.
- **Declared folders** are read-only. The app lists them in its own
  `package.json`, where they're visible in review and fixed at build time:

  ```json
  "chain": { "readOnlyFolders": ["~/.claude/projects", "~/.codex/sessions"] }
  ```

  Each entry is an absolute path or starts with `~/` (the user's home).
  An entry may also be a **single file** (Lazify's request 10:
  `~/.claude.json`, `~/.codex/auth.json`). That grants read-only access to
  that one file and nothing beside it, and `grants()` reports it as
  `kind: "file"`. The kind is what's on disk, so a declared path that
  doesn't exist yet is reported as `"folder"` until it appears; calls on
  it fail `NOT_FOUND` meanwhile.
  `chain dev` and `chain build` compile the list into the app. There's no
  runtime API to add one: a list the running app could extend would only
  be bookkeeping, not a boundary (the same reasoning as process-runner's
  "no caller-supplied allowlist"). A declared folder that doesn't exist
  yet (an agent that has never run) is still listed by `grants()`, and
  calls under it fail `NOT_FOUND` until it appears.

Mneme's request 11 is covered by declared folders plus `list()`,
`stat()` and `readBytes()` with an `offset`. It needs no separate
`external-files` capability.

## The app's own folders — `appFolder(kind)`

Lazify's request 06: places of the app's own whose real paths go into
other programs' arguments (`git --git-dir=<folder>`, `npm install` in a
staging folder, a pasted image's path typed into an agent CLI).
`desktop.files` can't do that: it hides paths on purpose.

`appFolder("data" | "temp")` returns a read-write grant
(`source: "app"`, id `app:data` / `app:temp`) that always exists:

| Kind   | macOS path                                          | Lifetime                                |
| ------ | --------------------------------------------------- | --------------------------------------- |
| `data` | `~/Library/Application Support/<identifier>/app`    | Survives restarts and updates.          |
| `temp` | `$TMPDIR/<identifier>` (`/private/var/folders/…/T/…`) | The OS may clear it (unused for ~3 days). |

- Everything in this contract works under them, as under a picked
  folder. They're valid `cwd`s for `process-runner` and `terminal`.
- Each is created when the app starts, and again by `appFolder()` if
  something deleted it (the OS clearing temp files).
- `data` is a folder **beside** the files Chain itself keeps there (the
  grants file, the database, `files/`), never their parent, so the app
  can't edit its own grants.
- They're private to the app and never inside the user's folders. They
  aren't listed by `grants()`, which stays "what the user or the build
  granted".
- There's no app-run or instance id. An app that runs several instances
  at once names its own subfolders (Lazify uses its own ids) and cleans
  up after itself.

## Paths

- Every path passed in must be **absolute**. A relative path, or one
  containing a `..` component, rejects with `INVALID_ARGUMENT`.
- Every path is resolved natively on every call: the longest existing
  part is canonicalised (symlinks resolved, `/var` → `/private/var`, the
  case corrected to what's on disk on case-insensitive volumes), and any
  part that doesn't exist yet is appended. The result must be inside a
  grant. A folder grant covers itself and everything under it; a file
  grant covers that one file.
- **Symlinks are followed only when the target is inside a grant too.**
  Reading, writing, listing and watching act on the target, so the
  *target* must be inside a grant. `stat()`, `move()` and `delete()` act
  on the link itself, so only the *link's* location must be inside a
  grant. A recursive `list()` reports symlinked folders but never enters
  them.
- Paths returned (in grants, entries and change events) are absolute and
  canonical. A path passed in through a symlink comes back as its real
  location.
- A grant's own root can't be moved or deleted (`INVALID_ARGUMENT`).
  Revoke the grant instead.

## Granting

### `pick(options?)`

Shows the OS folder picker, attached to the window (a sheet on macOS).
`multiple` allows more than one choice, and `files` allows files as well
as folders. Every chosen item becomes a read-write grant and is returned.
Cancel returns `[]`, not an error. Choosing something that is already
granted returns the existing grant instead of adding a duplicate.

### `onDrop(handler)`

While at least one handler is subscribed, anything the user drops onto
the window is granted read-write (`source: "dropped"`) and the new grants
are passed to every handler. With no handler, drops are ignored and
nothing is granted. Reloading the page drops every subscription, so a
page must subscribe again after a reload. Returns an unsubscribe function.

### `grants()` and `revoke(id)`

`grants()` returns every grant: picked and dropped ones (persisted in the
app's data folder) and the declared ones. `revoke(id)` forgets a picked or
dropped grant and is idempotent (an unknown id resolves). Revoking a
declared grant rejects with `INVALID_ARGUMENT`. Revoking doesn't stop a
running `watch()` that was started under that grant.

## Reading

- `list(path, { recursive?, skipFolders? })` returns the entries in a
  folder: `{ path, name, kind, size, modifiedMs }`. The order is
  unspecified, except that a recursive listing puts every folder before
  its contents. `skipFolders` names are reported but not entered. In a
  recursive listing, a subfolder that can't be read is skipped silently.
  Only the folder that was asked for reports errors.
- `stat(path)` returns the entry for one path, without following a final
  symlink.
- `exists(path)` resolves `true` or `false`. It rejects with `NOT_GRANTED`
  for a path outside every grant, so it can't be used to probe for files.
- `readText(path, { maxBytes? })` returns the file decoded as UTF-8. A
  file that isn't valid UTF-8 rejects with `INVALID_ARGUMENT`, and is
  never decoded lossily, so a read-modify-write can't silently corrupt
  it. Read it with `readBytes()` instead.
- `readBytes(path, { offset?, length?, maxBytes? })` returns up to
  `length` bytes starting at `offset`. An offset at or past the end
  returns an empty array. Together with `stat()`'s `size`, this is the
  incremental read Mneme's request 11 asked for. Lazify's 16 MB preview
  cap is `maxBytes`.
- `maxBytes` rejects with `TOO_LARGE` *before* reading anything.

## Writing

All writes need a read-write grant. A path under a read-only declared
folder rejects with `NOT_GRANTED`.

- `writeText(path, text)` and `writeBytes(path, bytes)` are **atomic**:
  the bytes go to a temporary file next to the target, which is flushed
  to disk and then renamed over it. After a crash, the file is either
  the old content or the new, never a mix of the two. An existing file keeps its
  permissions. Writing through a symlink replaces the target's content
  and leaves the link alone. The parent folder must already exist
  (`NOT_FOUND` otherwise).
- `createFolder(path)` creates the folder and any missing parents.
  Resolves if it already exists as a folder, and rejects with
  `INVALID_ARGUMENT` if a file is in the way.
- `move(from, to)` renames or moves a file, folder or symlink. Both paths
  must be inside read-write grants. It rejects with `INVALID_ARGUMENT` if
  `to` already exists. The exception is a case-only rename (`Readme.md` →
  `README.md`) on a case-insensitive volume, which is allowed. Moving
  across volumes is not supported and rejects with `NATIVE_FAILURE`.
- `delete(path, { toTrash? })` removes a file or a symlink (never its
  target), or a folder and everything in it. It's idempotent: a path that
  doesn't exist resolves. With `toTrash`, the item goes to the Trash
  instead (macOS only for now; `UNSUPPORTED` elsewhere). The app confirms
  destructive actions in its own UI.

## Watching

### `watch(path, onChange, { recursive? })`

Reports changes under `path`, a folder or a file inside a grant (a
read-only grant is enough). It resolves with a handle once the OS watcher
is running. `onChange` receives a batch of `{ path, kind }` changes. The
kind is `created`, `modified`, `removed`, `renamed`, or `other` when the
OS didn't say. A rename reports both the old and the new path when the OS
knows both. `rescan` means the OS lost track (a buffer overflow, or
events dropped under heavy churn), so anything under `path` may have
changed and the app should re-read what it cares about.

- A `path` that doesn't exist rejects with `NOT_FOUND`, so the app can
  skip it. It doesn't wait silently for the folder to appear.
- `handle.stop()` releases the OS watcher and is idempotent. Reloading
  the page stops every watch the page started.
- Events can arrive late or batched (FSEvents latency). There's no
  debouncing beyond what the OS does. The app keeps its own settle timers.

## Errors

| Code | When |
| --- | --- |
| `NOT_GRANTED` | The path is outside every grant, or it's a write under a read-only grant. |
| `PERMISSION_DENIED` | The OS refused, e.g. macOS privacy protection on `~/Documents` was denied. Never reported as `NOT_FOUND`. |
| `NOT_FOUND` | The path, or for a write its parent folder, doesn't exist. |
| `INVALID_ARGUMENT` | Relative path, a `..` component, wrong kind (reading a folder as a file, listing a file), invalid UTF-8 in `readText`, path too long, `move` onto an existing path, moving or deleting a grant root, revoking a declared grant. |
| `UNAVAILABLE` | The file is locked by another process (Windows sharing violations). |
| `TOO_LARGE` | The read would exceed `maxBytes`. |
| `UNSUPPORTED` | Outside a Chain runtime, or `toTrash` on a platform without it. |
| `NATIVE_FAILURE` | Anything else, including a cross-volume `move`. |

## Non-goals

- **No access outside grants**, and no way for the running app to widen
  its own access. Picks and drops come from the user; declared folders
  come from the build.
- **No `.gitignore` or any other ignore-file handling.** `skipFolders` is
  a plain list of names. What to skip is the app's decision.
- **No file-format knowledge**: nothing about `.env`, JSON, templates or
  encodings beyond UTF-8.
- **No settings stores** in the app folders: small JSON state belongs in
  `desktop.storage`.
- **No running processes** in these folders. That's `process-runner` and
  `terminal`, which check a `cwd` against these grants.
- **No glob filtering, content diffing or debouncing of changes.**
  Watches don't persist across restarts or reloads.
- **No copy.** Nothing asked for it yet. Lazify's template copy can read
  and write each file.
- **No OS security-scoped bookmarks.** Chain apps aren't sandboxed, so a
  grant is an app-level policy that Chain Core enforces. The grant id is
  opaque, so a sandboxed build could attach a bookmark to it later without
  changing the contract.
- **`desktop.files` is unchanged.** Its no-real-paths rule stays as it
  is.
