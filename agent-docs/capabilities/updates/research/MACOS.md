# Updates — macOS Research

Found live on macOS 27 (Apple Silicon), 17 October 2026.

- **Tauri's release build is linker-signed only** (`flags=0x20002(adhoc,linker-signed)`,
  `Identifier=<name>-<hash>`), designated requirement `cdhash H"…"` —
  different for every build. TCC (microphone, Apple Events, Files and
  Folders) keys grants to the designated requirement, so each update
  re-prompts for each.
- **A self-signed code-signing certificate** (`CSSMERR_TP_NOT_TRUSTED`
  in `security find-identity`, no trust settings) still signs:
  `codesign` and Tauri's `signingIdentity` accept it, and the
  designated requirement becomes `identifier "<bundle id>" and
  certificate leaf = H"<sha1>"`. TCC grants survived an update signed
  with the same certificate. Signing with an identity turns on the
  hardened runtime (`flags=0x10000(runtime)`); the
  `com.apple.security.device.audio-input` / `automation.apple-events`
  entitlements from `chain.permissions` are needed and were present.
  `spctl --assess` says "rejected" (not notarized), which only matters
  for quarantined files.
- **Files fetched by the app (reqwest) get no `com.apple.quarantine`**,
  so a swapped-in update opens without a Gatekeeper check.
- **Open-panel choices are recorded on the item** (`com.apple.macl`
  xattr) and survived ad-hoc updates.
- **tauri-plugin-updater 2.12**: `Update::download` verifies the minisign
  signature (and, with `requireSignedVersion`, the trusted comment's
  `version:`); Tauri CLI 2.12's `createUpdaterArtifacts` writes that
  version. Its macOS `install` renames the app into a `TempDir` that's
  dropped at the end (a failed second rename loses the app) and falls
  back to an AppleScript admin prompt — Chain doesn't use it.
- `renamex_np(RENAME_SWAP)` exchanges two directories atomically on APFS
  (same volume: the staging dir sits beside the bundle).
- WebKit holds `getUserMedia` while the page isn't visible
  (`visibilityState: hidden`), even with permission granted.
