# Files Capability — Contract

## What this is

Managed local blob storage for an app's own binary data (images,
attachments, exported files, ...) — conceptually parallel to what
`desktop.storage` already does for structured data, but for raw bytes
that don't belong in a SQL column. Backs things like mneme's
`attachment` table (`file_name`/`file_path`/`mime_type`), which already
expects a real on-disk file — the app-level schema and UI around
attachments is app-level and not part of this capability (see
Non-goals).

## `desktop.files.write(bytes, options?)`

```
write(bytes: Uint8Array, options?: { extension?: string }): Promise<string>
```

Writes `bytes` into the app's own managed storage directory under a
**capability-generated reference** — never the caller's own filename or
any path. The returned string is opaque to the app; store it (e.g. in
`attachment.file_path`) and pass it back verbatim to `read`/`url`/
`delete`. `options.extension` (letters/digits only, no leading dot) is
included in the generated reference so a stored image keeps a
recognizable extension for `url()` — it does not let the caller choose
the on-disk name itself.

The managed directory is created, and the per-user data directory
created if missing, the first time any `desktop.files.*` call is made —
same "no separate open step" behavior as `desktop.storage`.

## `desktop.files.read(reference)`

```
read(reference: string): Promise<Uint8Array>
```

Reads back the bytes previously returned by `write()`. Rejects with
`ChainError { code: "NOT_FOUND" }` if `reference` doesn't correspond to
an existing file (including one already deleted).

## `desktop.files.url(reference)`

```
url(reference: string): Promise<string>
```

Resolves to a URL usable directly as an `<img src>`/`<a href>` in the
webview (via Tauri's asset-protocol `convertFileSrc`), without loading
the file's bytes into JS memory. Same `NOT_FOUND` behavior as `read` for
a reference that doesn't exist.

## `desktop.files.delete(reference)`

```
delete(reference: string): Promise<void>
```

Deletes the file. **Idempotent** — deleting a reference that doesn't
exist (already deleted, or never existed) resolves successfully rather
than rejecting; the caller's intent ("this shouldn't exist anymore") is
already satisfied.

## `desktop.files.pick(options?)`

```
pick(options?: { multiple?: boolean; extensions?: string[] }):
  Promise<{ name: string; size: number; bytes: Uint8Array }[]>
```

Shows the OS's own open-file picker and resolves with each chosen file's
**name, size, and bytes** — never its path. Resolves `[]` if the user
cancels. Driven by mneme's request 16: an `<input type="file">` in the
webview opens as a free-floating app-modal window (wry calls
`runModal()`), while this one is attached to the app window — a sheet on
macOS, an owned dialog on Windows — and never blocks the UI thread while
it's open.

- `multiple` (default `false`) allows selecting more than one file.
- `extensions` limits which files can be chosen (letters/digits, no
  leading dot, like `write()`'s `extension`). Omitted or empty: any file.
- Picking is only an OS-level action. Nothing is stored in the managed
  directory; an app that wants to keep a picked file passes its bytes to
  `write()`.
- Only one picker at a time per app.

## `desktop.files.save(bytes, options?)`

```
save(bytes: Uint8Array, options?: { suggestedName?: string; extensions?: string[] }):
  Promise<{ name: string } | null>
```

The save counterpart of `pick()` (mneme request 17): shows the OS save
panel attached to the app window (a sheet on macOS), and the **native
side writes `bytes`** to the location the user chose. Resolves with the
file name the user settled on — never the path — or `null` on cancel.

- **Every call shows the panel.** Nothing is ever saved silently to a
  remembered or default location, and there's no "don't ask again". The
  panel may start in the folder the OS last used; the user confirms each
  time. (The user's explicit requirement.)
- `suggestedName` pre-fills the name field. Only its last path component
  is used, so it can't pick a folder.
- `extensions` works like `pick()`'s. If the user's name ends in none of
  them, the first is appended.
- Overwriting an existing file is confirmed by the panel's own "Replace?"
  prompt; the capability adds nothing on top.
- Shares `pick()`'s one-panel-at-a-time rule.

## Errors

- A `reference` that isn't a well-formed capability-generated id (or that
  otherwise doesn't resolve to an existing file for `read`/`url`)
  rejects with `ChainError { code: "NOT_FOUND" }`.
- Other I/O failures (disk full, permission denied by the OS, ...) reject
  with `ChainError { code: "NATIVE_FAILURE" }`, the underlying OS message
  in `message`.
- `pick()`/`save()` reject with `ChainError { code: "INVALID_ARGUMENT" }`
  if an `extensions` entry isn't letters/digits, and with `ChainError {
  code: "UNAVAILABLE" }` if a picker or save panel is already open.
  `save()` rejects with `NATIVE_FAILURE` (the OS message in `message`)
  if writing to the chosen location fails. A picked file that can't
  be read (removed or unreadable between choosing and reading) rejects
  with `NATIVE_FAILURE`.
- Calling any method outside a Chain (Tauri) runtime rejects with
  `ChainError { code: "UNSUPPORTED" }`, same as every other capability.

## Non-goals

- **Never accepts or returns a real filesystem path.** Only an opaque
  reference string the capability itself generated — this is deliberate
  (see the Windows `MAX_PATH` risk in `research/WINDOWS.md`), not an
  oversight to "fix" later by exposing the real path.
- No directory listing / enumeration API — the consuming app tracks its
  own references (e.g. mneme's `attachment` table); this capability only
  knows how to store and retrieve bytes by a reference it already handed
  out, not how to discover what's stored.
- No streaming API — whole-file read/write only. Add a streaming variant
  only when a real app hits an actual size/memory problem, not
  speculatively.
- No metadata (size, mime type, created-at) returned by any method — the
  consuming app already tracks whatever metadata it needs (mneme's
  `attachment.mime_type` column, for instance) at the app level.
- **`save()` has no "reveal in Finder"** (request 17's optional half) —
  add one only when an app needs it, without exposing the path.
- **`pick()` never returns a path, and has no folder
  picking or "remember last folder" option** — only what request 16
  needs. It also has no size limit or streaming: bytes are read whole
  (mneme caps attachments before reading). Add any of these only when a
  real app needs them.
- No cross-file transaction/atomicity guarantees — each call is
  independent, same as `storage.execute()` before a transaction API
  exists there.
