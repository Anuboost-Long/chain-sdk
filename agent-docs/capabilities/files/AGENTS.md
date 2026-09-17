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

## Status

Implemented and unit-tested on macOS:

- `crates/core/src/files.rs` has passing unit tests
  (`write_read_delete_round_trip`, `rejects_path_traversal_references`).
- Wired as real Tauri commands (`files_write`/`files_read`/
  `files_resolve_path`/`files_delete`) in `packages/cli/templates/lib.rs`
  (propagates to every `chain init`/`chain update`'d app) and in
  `apps/playground`.
- Not yet exercised through a real running app's webview end to end (no
  live probe run yet at the time this file was last touched) — see the
  README's "How to use it" for the exact call shape once that
  verification happens, and update this section when it does.

## What's NOT done yet (next steps for an agent to pick up)

- [ ] Run a real end-to-end probe in mneme (or `apps/playground`): write
      bytes from the actual webview, read them back, and render via
      `url()` — mirror how `storage` was verified (see its AGENTS.md).
      Revert the probe afterward.
- [ ] Verify on Windows — do NOT mark the contract/component status
      stable until confirmed there (rule: no single-platform contracts).
      See `research/WINDOWS.md` for the specific risks (MAX_PATH,
      antivirus locking) and the verification checklist.
- [ ] Add contract tests under `capabilities/files/tests/` — currently
      the only test is `crates/core/src/files.rs`'s Rust unit tests,
      same gap `storage` currently has too.
- [ ] Propagate to mneme via `chain update` and confirm mneme can
      actually start writing real `attachment.file_path` values instead
      of base64-inlining images (that migration of mneme's own
      `page-image.ts`/`course-image.ts` is mneme's job, not this
      capability's, but it's the actual point of building this).

## Rules specific to this capability

- Don't add a directory-listing/enumeration method, metadata fields
  (size, mime type, timestamps), or a streaming API speculatively — each
  is a deliberate non-goal until a real app hits the need (see
  CONTRACT.md).
- Never let a caller-provided filename or path reach `std::fs` — every
  path that touches disk must go through `is_valid_reference()` first
  (either freshly generated by `write()`, or validated on the way in for
  `read`/`url`/`delete`).
