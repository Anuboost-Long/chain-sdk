# Updates Capability — Agent Memory

Scope: `desktop.updates` — a built app updating itself. Requested by
Lazify (request 22). Electron's Squirrel.Mac never installed into
Lazify's ad-hoc builds; this has to.

## What's already decided

- **Integrity = the owner's Ed25519 key** (tauri-plugin-updater's
  minisign check), never Apple's signature. `requireSignedVersion` on.
- **Chain installs, not the plugin** (atomic swap; no admin prompt).
  Never call the plugin's `install`/`download_and_install`.
- **Every step is the app's call**; installing on a normal quit is the
  one automatic thing (Lazify asked for it, like electron-updater's
  autoInstallOnAppQuit).
- **No admin elevation**: an app this user can't replace is `read-only`.
- **Stable signing is the permissions fix**, offered as
  `CHAIN_SIGNING_IDENTITY`; ad-hoc stays the default.
- Keys and certificates never go in an app's repo (`chain updates key`
  refuses paths inside it).

## Status

- [x] macOS 27 (Apple Silicon), 17 Oct 2026: a `chain init` test app
      built release with `chain build` (+ the dev inspector to drive it),
      installed in ~/Applications, fed from a local http feed:
      - 0.1.0 → 0.1.1 by **install-on-quit** (menu Quit); 0.1.1 → 0.1.2
        and 0.1.3 → 0.1.4 by `installAndRelaunch()` (new pid, new
        version); nothing left beside the app; no quarantine flag.
      - `check()` → version/date/notes; `download()` progress (40 events,
        14.4 MB); `status().downloaded`.
      - tampered archive → `INTEGRITY_FAILED`; feed claiming 0.1.9 with
        0.1.2's archive → `INTEGRITY_FAILED` (signed-version mismatch);
        missing/malformed feed → `UNAVAILABLE`; a copy in a read-only
        folder → `read-only`; feed with two builds of one version merges.
      - permissions (a person clicking the prompts): ad-hoc 0.1.0 granted
        mic, Apple Events (Finder), Documents → after updating, all three
        prompted again. Self-signed-certificate 0.1.3 granted all three →
        0.1.4 (same certificate) kept all three, no prompts. A picked
        folder kept working across ad-hoc updates (`com.apple.macl`).
      - The test app, its data and privacy entries, and the throwaway
        certificate were removed afterwards.
- [ ] Not verified: a first launch from a quarantined, browser-downloaded
      copy (Apple's rule documented, not re-tested); `dev-build` status
      under a real `chain dev` (unit-tested logic only); a universal
      build; an https feed (GitHub releases); disk-full during install;
      Lazify's own use.
- [x] Propagated to Lazify with `chain update`, 17 Oct 2026 (ticket
      044): native code compiles; in a live `chain dev` with no
      `chain.updates`, `status()` and `check()` report `not-configured`
      (it's checked before `dev-build`). Feed, key and the self-signed
      certificate are Lazify's owner's decisions.
- [ ] Windows: not started (`unsupported-platform`).
