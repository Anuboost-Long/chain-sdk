# Packages Capability — Contract

## What this is

Downloaded archives the app **runs from**: a VS Code extension whose
language server the app starts with the user's own `java` or `node`.
Requested by Lazify (request 17): SonarLint (177 MB) and Tailwind CSS
IntelliSense (1.8 MB) from Open VSX, installed only to run the language
server each one ships.

It is `desktop.models`' install machinery (streamed to disk, sha256
checked, safe unpack, atomic replace, cancel) with the two things models
deliberately refuses: the app gets the **paths**, and what's downloaded
is meant to be **run**. Models stays data-only; that is why this is a
separate capability (user decision, 2026-10-14). The app owns the
catalog (registry lookups, URLs, hashes, versions).

## `desktop.packages.install(manifest, onProgress?)`

```
install(
  { id, url, sha256, archive?, sizeBytes? },
  onProgress?: (received: number, total: number | null) => void,
): Promise<{ id; path; sizeBytes; installedAt }>
```

- `id` — app-chosen, 1–64 of `a–z`, `0–9`, `-`. One folder per id.
- `url` — `https://` only.
- `sha256` — of the downloaded file. Streamed to a temp file and hashed;
  on mismatch it's deleted and `install` rejects `INTEGRITY_FAILED`.
- `archive` — `zip`, `tar.gz`, `tar.bz2` or `none`; omitted, inferred
  from the URL's extension (else `none`). A `.vsix` is a zip but isn't
  inferred as one: pass `archive: "zip"`.
- The archive is unpacked **exactly as it is laid out**: no top folder
  is lifted out (unlike models), so `extension/server/x.jar` in the
  archive is `<path>/extension/server/x.jar`.
- Only plain files and folders. A symlink, hardlink, device entry,
  absolute path or `..` path rejects the whole install with
  `INVALID_ARGUMENT`, and nothing is kept. Files get default permissions:
  **nothing is made executable**. A package is run through a program the
  user already has (`java -jar`, `node`), never as a binary of its own.
- Reinstalling an id replaces it atomically: the old copy stays usable
  until the new one is fully unpacked, so there's never a mix of old and
  new files. A failed or cancelled install leaves nothing behind.
- `onProgress` fires about every 100 ms and once at the end. `total` is
  the server's Content-Length, else `sizeBytes`, else `null`.
- **Stall timeout:** no data for 30 seconds rejects `UNAVAILABLE`.
- One install per id at a time; a second concurrent `install` of the
  same id rejects `UNAVAILABLE`. Different ids install in parallel.
- `path` is the package's absolute folder; `sizeBytes` its size on disk.

## `desktop.packages.resolve(id, file)`

The absolute path of `file` (relative, `/`-separated, no `..`) inside an
installed package. Rejects `NOT_FOUND` when the package or that file
isn't there, so it doubles as "is the server file there after
installing?". The path goes straight into `processRunner.run()`'s argv:

```ts
const jar = await desktop.packages.resolve("sonarlint", "extension/server/sonarlint-ls.jar");
await desktop.processRunner.run("java", ["-jar", jar, "-stdio"], onOutput, { output: "bytes", keepStdinOpen: true });
```

## `cancel(id)` / `list()` / `remove(id)`

- `cancel(id)` — the running install of `id` rejects `CANCELLED` and its
  partial files are removed. No-op when none is running.
- `list()` — every installed package (`{ id, path, sizeBytes,
  installedAt }`), sorted by id.
- `remove(id)` — deletes the package. Idempotent. Cancels a running
  install of the same id first. A process still running from the
  package isn't stopped; stop it first.

## Where packages live

`<app data>/packages/<id>/`, beside `models/` and `files/` — not inside
`appFolder("data")`. The folder isn't a `desktop.folders` grant, so it
can't be a process-runner `cwd` or read through `desktop.folders`;
nothing asked for that. Paths go into argv.

## Errors (`ChainErrorCode`)

- `INVALID_ARGUMENT` — bad id, non-https URL, malformed hash, unsafe
  archive entry, unreadable zip, a `file` that isn't a plain relative path.
- `INTEGRITY_FAILED` — the download doesn't match `sha256`.
- `UNAVAILABLE` — network failure, a non-2xx response, a 30 s stall, or
  the id is already installing.
- `CANCELLED` — after `cancel(id)` or `remove(id)`.
- `NOT_FOUND` — from `resolve`: the package or file isn't installed.
- `NATIVE_FAILURE` — disk errors and anything else.
- `UNSUPPORTED` — outside a Chain (Tauri) app.

## Non-goals

- **No running.** This capability never starts anything; the app does,
  with `processRunner`.
- **No executable bits, no native binaries.** Nothing is chmod'ed `+x`,
  and a package that needs its own native executable is out of scope:
  macOS would also quarantine-check it, and nothing asked for it.
- No signature checks beyond sha256, no registry/catalog, no update
  checks — the app owns those.
- No resume of interrupted downloads (a retry starts over).
- No configurable stall timeout or progress interval.
