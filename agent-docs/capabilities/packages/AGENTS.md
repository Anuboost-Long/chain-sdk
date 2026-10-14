# Packages Capability — Agent Memory

Scope: `desktop.packages` — install/resolve/cancel/list/remove archives
the app runs from. Requested by Lazify
(`docs/chain-sdk-requests/17-install-a-downloaded-package.md` in the
lazify-chain repo): VS Code extensions from Open VSX, for the language
servers they ship.

## What's already decided

- **Separate from `models`** (user decision, 2026-10-14). Models
  promises data only and no paths; that promise stays. This capability
  is the same store code (`models::Models::open_store(dir, false)`)
  wrapped in `crates/core/src/packages.rs`, so a fix to download or
  unpack lands in both.
- **Paths are returned**, and archives are laid out exactly as they are
  (no single-top-folder lift), because the app builds argv from archive
  paths (`extension/server/sonarlint-ls.jar`).
- **Nothing is made executable, nothing is run here.** The app runs a
  package through a program the user already has (`java -jar`, `node`)
  with `processRunner`. A package needing its own native binary is out of
  scope (and would hit Gatekeeper's quarantine checks).
- **An unsafe entry rejects the whole install** (as in models), where
  Electron Lazify skipped it. Safer, and real VSIX files have none.
- Packages live in `<app data>/packages/`, not under `appFolder("data")`,
  so they aren't a `desktop.folders` grant: no `cwd`, no `folders.read`.
  `resolve(id, file)` covers "is the server file there?".
- Stall timeout: 30 s without data (`reqwest` `read_timeout`), shared
  with models.

## Status

- [x] macOS arm64: `cargo test -p chain-core models packages`
      (unlifted zip layout, no exec bit from a 0755 zip entry, resolve
      errors) plus models' hostile-archive tests. End to end in a
      throwaway `chain init` app under `chain dev`, through the SDK,
      14 October 2026, against Open VSX: Tailwind CSS IntelliSense 0.16.0
      (1,762,070 bytes, published sha256) installed in 3.9 s with progress
      ending at `[1762070, 1762070]`; `resolve` gave
      `…/packages/tailwind/extension/dist/tailwindServer.js`, mode
      `-rw-r--r--`; missing file `NOT_FOUND`; `../models`
      `INVALID_ARGUMENT`; wrong sha256 `INTEGRITY_FAILED`; `http://`
      `INVALID_ARGUMENT`; reinstall replaced it; SonarLint 6.1.0 cancelled
      at 5 MB of 176,650,008 with `CANCELLED`; remove twice, then
      `NOT_FOUND`. Afterwards `packages/` held only empty `.meta`,
      `.partial`, `.trash`.
- [x] In Lazify itself (macOS 27.0.1, Lazify commit 685bbb9, after
      `chain update`): Tailwind 0.16.0 in 3.0 s, SonarLint 6.1.0
      (176,650,008 bytes) in 128 s with progress every ~100 ms and ~2.5 s
      unpacking, both against Open VSX's sha256 with `archive: "zip"`;
      `resolve` found each server file; remove → `NOT_FOUND` → reinstall.
- [ ] The 30 s stall timeout has no live or unit test (it's reqwest's
      `read_timeout`).
- [ ] Windows: never built or run. Plain Rust, nothing per-OS, but
      long paths inside a VSIX (`node_modules/…`) may pass `MAX_PATH`;
      see research/WINDOWS.md.
- [ ] Contract tests under `capabilities/packages/tests/`.

## Rules specific to this capability

- Never mark anything executable, and never start anything from here.
- Never add a runnable mode to `desktop.models` instead; this is the
  place for runnable archives.
- No registry/catalog logic: the app talks to Open VSX itself.
