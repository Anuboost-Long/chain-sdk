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

## Errors

- A `reference` that isn't a well-formed capability-generated id (or that
  otherwise doesn't resolve to an existing file for `read`/`url`)
  rejects with `ChainError { code: "NOT_FOUND" }`.
- Other I/O failures (disk full, permission denied by the OS, ...) reject
  with `ChainError { code: "NATIVE_FAILURE" }`, the underlying OS message
  in `message`.
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
- No cross-file transaction/atomicity guarantees — each call is
  independent, same as `storage.execute()` before a transaction API
  exists there.
