# Files Capability — Agent Memory

Scope: `desktop.files.write/read/url/delete` — managed local blob
storage. Requested by mneme (see
`docs/chain-sdk-requests/02-files.md` in the mneme repo) as its second
real capability gap: images/attachments are currently base64-inlined
into the page `content` TEXT column as a workaround, and the
`attachment` table's `file_path` column has sat unused because there was
nothing to write a real file to.

Read order for a task in this capability:

1. Root `/AGENTS.md`
2. `/docs/ARCHITECTURE.md`
3. This file
4. `CONTRACT.md` + `contract.ts`
5. `research/MACOS.md` / `research/WINDOWS.md`
6. Relevant source in `crates/core/src/files.rs` / `packages/sdk/src/files.ts`

## What's already decided

- One managed directory per app (`<app_data_dir>/files/`), a sibling of
  `storage`'s `app.db`, created lazily on first `write`/`read`/`url`/
  `delete` call — no separate "open" step in the public API, matching
  `storage`'s own convention.
- **The app never sees or provides a real filesystem path** — only an
  opaque, capability-generated reference string (a short hex id, plus an
  optional extension). This isn't a style choice; it's the direct
  mitigation for the Windows `MAX_PATH` risk the request document flagged
  and for path-traversal safety (`is_valid_reference()` in
  `crates/core/src/files.rs` rejects anything that isn't
  alphanumeric-or-dot).
- Reference ids are generated from OS-seeded randomness
  (`std::collections::hash_map::RandomState`, the same source `HashMap`
  uses internally) hashed with the current time — deliberately not a new
  `uuid`/`rand` crate dependency, matching `storage`'s own "no dependency
  beyond what's already needed" precedent. `write()` retries (up to 5x)
  on the astronomically unlikely event of a collision rather than
  assuming it can't happen.
- `delete()` is idempotent (deleting an already-gone or never-existing
  reference succeeds); `read()`/`url()` reject with `NOT_FOUND` for the
  same case — different behavior for a real reason, see CONTRACT.md.
- `url()` doesn't do anything Tauri-specific on the Rust side — Rust only
  resolves the reference to an absolute path (`files_resolve_path`, an
  internal Tauri command, not part of the public contract); the SDK
  wrapper (`packages/sdk/src/files.ts`) applies `convertFileSrc()` itself,
  since that's `@tauri-apps/api/core` surface the SDK is allowed to touch
  directly (same boundary `storage.ts`/`platform.ts` already sit at).
- **No new `capabilities/default.json` permission entry needed** — verified
  by checking how `storage`'s own custom commands work today: app-defined
  `#[tauri::command]`s aren't gated by the plugin ACL/permission system,
  only plugin-exposed commands are. The original request assumed this
  capability would need one; it doesn't, under the current
  custom-Tauri-command approach (not a Tauri filesystem plugin).
- Byte payloads cross the Tauri IPC boundary as a plain JSON array of
  numbers (`Vec<u8>` Rust-side) — confirmed empirically that
  `JSON.stringify` on a raw `Uint8Array` produces an *object* with
  numeric string keys, not a JSON array, so `files.ts` explicitly does
  `Array.from(bytes)` before `invoke()`; skipping that step would silently
  fail to deserialize into `Vec<u8>` on the Rust side.
- **`url()` needs two more things beyond the app's own permission ACL** —
  found the hard way (see "A real bug" below): the `tauri` dependency
  needs the `"protocol-asset"` Cargo feature (not a default feature —
  without it, the `asset://` URI scheme handler is compiled out of the
  binary entirely, a missing-handler problem, not a config one), and
  `tauri.conf.json` needs `app.security.assetProtocol.enable: true` plus
  a `scope` glob covering `$APPDATA/files/*` (Tauri's asset protocol
  defaults to serving nothing). Both are patched automatically by
  `patchCargoToml()`/`patchTauriConf()` in `packages/cli/src/scaffold.ts`
  — an app never needs to set this up by hand.

### A real bug this capability shipped with, and how it was found

The first "verified end to end" pass (see git history) only checked that
`write()`/`read()`/`delete()` round-tripped bytes correctly and that
`url()` produced a well-formed string — it never actually loaded the
resulting URL as an image. mneme-83 (a peer session working on mneme)
caught the real failure by pasting an actual image in mneme's live
window: `img.naturalWidth`/`naturalHeight` came back `0`, and
`fetch(img.src)` threw `TypeError: Load failed` — WebKit's signature for
a custom-scheme handler refusing a request outright, not a 404. Re-tested
here against `apps/playground` with the two fixes above: an actual
`<img>` element (not just a checked-but-unrendered `Image()` object) was
screenshotted showing a real loaded picture, and `fetch()` returned `200`
with `content-type: image/png`. **Lesson for next time: "the URL string
looks right" is not the same claim as "the image loads" — verify the
literal thing the capability promises, rendered, not just its inputs and
intermediate values.**

## Status

Implemented and verified for real on macOS:

- `crates/core/src/files.rs` has passing unit tests
  (`write_read_delete_round_trip`, `rejects_path_traversal_references`,
  `write_without_extension_has_no_dot`).
- Wired as real Tauri commands (`files_write`/`files_read`/
  `files_resolve_path`/`files_delete`) in `packages/cli/templates/lib.rs`
  (propagates to every `chain init`/`chain update`'d app) and in
  `apps/playground`.
- Propagated to `mneme` via `chain update` itself — `.chain/native/src/lib.rs`
  picked up the four new commands with no conflicts; a second `chain
  update` afterward correctly reported "Already up to date."
- Verified end to end in `apps/playground`'s actual running window (raw
  `tauri dev --features chain-dev-inspector`, not `chain dev` — playground
  intentionally keeps the old `src-tauri` layout, so `chain dev`'s
  `checkChainApp` gate doesn't apply to it; a hand-rolled client speaking
  the dev-inspector's TCP protocol directly, since `chain inspect` itself
  also gates on `checkChainApp`). Two passes: the first only checked
  `write()`/`read()`/`delete()`'s byte-exactness and that `url()` produced
  a well-formed string — it missed the asset-protocol bug below entirely.
  The second pass, after that fix, rendered an actual `<img>` in the page
  and confirmed via a real screenshot (a visible loaded picture, not a
  broken-image icon) plus `fetch(url)` returning `200`/`image/png`. The
  probe and the temporary dev-port bump (to avoid colliding with another
  already-running dev server on the default port) were reverted after
  each pass — `git diff --stat apps/playground/` is clean.
- **Asset-protocol bug found and fixed** — see "A real bug this
  capability shipped with" above. Fixed in `packages/cli/src/scaffold.ts`
  (`patchCargoToml`/`patchTauriConf`) and manually mirrored in
  `apps/playground`; propagated to `mneme` via `chain update`
  (`.chain/native/tauri.conf.json` and `.chain/native/Cargo.toml` both
  updated, no conflicts). mneme's own already-running dev server (a peer
  session's, left untouched) will need a restart to pick up the
  Cargo.toml feature change — a running `cargo` process won't recompile
  that on its own file-watcher.

## What's NOT done yet (next steps for an agent to pick up)

- [ ] Verify on Windows — do NOT mark the contract/component status
      stable until confirmed there (rule: no single-platform contracts).
      See `research/WINDOWS.md` for the specific risks (MAX_PATH,
      antivirus locking) and the verification checklist.
- [ ] Add contract tests under `capabilities/files/tests/` — currently
      the only test is `crates/core/src/files.rs`'s Rust unit tests,
      same gap `storage` currently has too.
- [ ] mneme actually switching `page-image.ts`/`course-image.ts` from
      base64-inlining to real `desktop.files.write` calls, and populating
      `attachment.file_path` with the result — this capability already
      reached mneme via `chain update`; the app-level migration to use it
      is mneme's job, not this capability's, but it's the actual point of
      having built this.
- [ ] Confirm mneme's paste-an-image repro (the one that originally
      surfaced the asset-protocol bug) actually loads now, after mneme's
      dev server is restarted to pick up the new Cargo feature — not yet
      confirmed there specifically, only in `apps/playground`.

## Rules specific to this capability

- Don't add a directory-listing/enumeration method, metadata fields
  (size, mime type, timestamps), or a streaming API speculatively — each
  is a deliberate non-goal until a real app hits the need (see
  CONTRACT.md).
- Never let a caller-provided filename or path reach `std::fs` — every
  path that touches disk must go through `is_valid_reference()` first
  (either freshly generated by `write()`, or validated on the way in for
  `read`/`url`/`delete`).
