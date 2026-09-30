# Models Capability — Contract

## What this is

Optional, user-downloaded **data-only** model packs — ONNX weights, token
lists, lexicons — that engines bundled in the app run by id. Requested by
mneme (request 21): open-source models offered as downloads the user
chooses and can remove, instead of shipping inside the app. The app owns
the catalog (URLs, hashes, which files are which); Chain installs only the
manifest it's handed.

Engines using models today: `desktop.speech.transcribe(…, { engine })`
(sherpa-onnx ASR). TTS is not built yet (see Non-goals).

## `desktop.models.install(manifest, onProgress?)`

```
install(
  { id, url, sha256, archive?, sizeBytes? },
  onProgress?: (received: number, total: number | null) => void,
): Promise<{ id; sizeBytes; installedAt }>
```

- `id` — app-chosen, 1–64 of `a–z`, `0–9`, `-`. One folder per id.
- `url` — `https://` only.
- `sha256` — of the downloaded file. The file is streamed to a temp file
  and hashed; on mismatch it's deleted and `install` rejects
  `INTEGRITY_FAILED`. Nothing is kept from a failed install.
- `archive` — `tar.bz2`, `tar.gz`, `zip` or `none`; omitted, it's inferred
  from the URL's extension (else `none`). With `none` the file is stored
  under the URL's last path segment.
- Extraction accepts **only plain files and folders**. A symlink,
  hardlink, device entry, absolute path or `..` path rejects the whole
  install with `INVALID_ARGUMENT`. Files are written with default
  permissions; nothing is made executable, and nothing downloaded is ever
  run.
- An archive whose content sits in one top folder has that folder lifted
  out, so catalog file names are relative to the model itself
  (`tokens.txt`, not `sherpa-onnx-…/tokens.txt`).
- Installing an id that's already installed replaces it — atomically: the
  old copy stays usable until the new one is fully extracted.
- `onProgress` fires about every 100 ms and once at the end. `total` is
  the server's Content-Length, else `sizeBytes`, else `null`.
- One install per id at a time; a second concurrent `install` of the same
  id rejects `UNAVAILABLE`. Different ids may install in parallel.
- `sizeBytes` in the result is the size on disk after extraction.

## `desktop.models.cancel(id)` / `list()` / `remove(id)`

- `cancel(id)` — the running install of `id` rejects `CANCELLED` and its
  partial files are removed. No-op when none is running.
- `list()` — every installed model, sorted by id.
- `remove(id)` — deletes the model. Idempotent. Cancels a running install
  of the same id first.

Paths are never returned. Engines take the id plus relative file names.

## Errors (`ChainErrorCode`)

- `INVALID_ARGUMENT` — bad id, non-https URL, malformed hash, unsafe
  archive entry, unreadable zip.
- `INTEGRITY_FAILED` — the download doesn't match `sha256`.
- `UNAVAILABLE` — network failure, a non-2xx response, or the id is
  already installing.
- `CANCELLED` — after `cancel(id)` or `remove(id)`.
- `NOT_FOUND` — from an engine: the model or a named file isn't installed.
- `NATIVE_FAILURE` — disk errors and anything else.

## Non-goals

- No catalog, search or update checks — the app owns those.
- No resume of interrupted downloads (a retry starts over).
- **No downloaded code, ever** — engines are compiled into the app and
  signed with it; packs are data only.
- No TTS yet. sherpa-onnx's TTS voices need espeak-ng (GPL-3); see
  `research/LICENSING.md` before adding it.
