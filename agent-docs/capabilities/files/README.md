# Files Capability (`desktop.files`)

## How it works

Managed local blob storage — one directory per app (`files/`, a sibling
of `storage`'s `app.db`), opened lazily on first use in the OS's
per-user app-data directory (resolved via Tauri's
`app_handle.path().app_data_dir()`, the exact same call `storage` uses).
`crates/core/src/files.rs` is plain `std::fs` — identical on every
platform, no per-OS Rust code, same reasoning `storage` established for
`rusqlite`.

The public shape never exposes a real filesystem path: `write(bytes,
options?)` generates its own short reference (a random hex id, plus an
optional extension) and hands that back — the app never picks or even
sees the on-disk location. `read(reference)` and `url(reference)` read
it back or resolve a webview-usable URL (via Tauri's asset-protocol
`convertFileSrc`, applied in the SDK wrapper); `delete(reference)`
removes it and is safely callable twice. This split exists specifically
to avoid ever writing a long, user-controlled filename to disk — a real
`MAX_PATH` risk on Windows — and to make path traversal structurally
impossible (`is_valid_reference()` rejects anything that isn't a plain
alphanumeric-plus-dot string).

Requested by mneme as its second real capability gap beyond `storage` —
see `docs/chain-sdk-requests/02-files.md` in the mneme repo for the
original ask and native-module survey. mneme's actual problem: images
were being base64-inlined directly into a SQLite TEXT column (a ~33%
size inflation, and every read of that row's content paid for the image
bytes even when not displaying it) because there was nothing to write a
real file to — the `attachment` table's `file_path` column had sat
unused since it was scaffolded.

## How to use it

```ts
import { desktop } from "@chain/sdk";

const bytes = new Uint8Array(await file.arrayBuffer());
const reference = await desktop.files.write(bytes, { extension: "png" });
// store `reference` yourself, e.g. attachment.file_path = reference

const src = await desktop.files.url(reference);
// <img src={src} />

const backAgain = await desktop.files.read(reference);

await desktop.files.delete(reference); // idempotent
```

Every already-scaffolded app gets this automatically via `chain update`
(it's a tracked file in `packages/cli/templates/lib.rs`) — no manual
wiring needed per app.

## Files to check

- `agent-docs/capabilities/files/CONTRACT.md` — the semantic contract
  (API behavior, error model, explicit non-goals — especially "never a
  real path"). Check this before changing behavior or adding a method.
- `capabilities/files/contract.ts` — the exact types (`FilesApi`);
  change this and both implementations below together, never one
  without the others.
- `agent-docs/capabilities/files/AGENTS.md` — the actual TODO checklist
  (real end-to-end verification, Windows, contract tests) and what's
  already decided and why.
- `agent-docs/capabilities/files/research/MACOS.md` /
  `agent-docs/capabilities/files/research/WINDOWS.md` — platform-specific
  findings; Windows is explicitly **not yet verified** (no Windows
  machine was available) — read the checklist there before assuming it
  works.
- `crates/core/src/files.rs` — the actual Rust implementation
  (`Files::open/write/read/delete/resolve`, `is_valid_reference()`,
  `generate_id()`). Has real unit tests (`cargo test -p chain-core`) —
  extend them here rather than only testing through the Tauri layer.
- `packages/sdk/src/files.ts` — SDK-side wrapper; handles the
  `isTauri()` check, wraps native failures into `ChainError` (including
  mapping the `"NOT_FOUND: "`-prefixed Rust error onto
  `ChainErrorCode.NOT_FOUND`), converts the `Uint8Array` payload to a
  plain number array before `invoke()` (required — see AGENTS.md for
  why), and applies `convertFileSrc()` for `url()`.
- `packages/cli/templates/lib.rs` — the Tauri command layer
  (`files_write`/`files_read`/`files_resolve_path`/`files_delete`,
  lazy-open via `FilesState`) that every scaffolded app gets. This is
  the file `chain update` propagates — see
  `agent-docs/framework/command/README.md`.
- `apps/playground/src-tauri/src/lib.rs` — same wiring, kept in sync by
  hand (playground isn't `chain init`-managed) so the framework's own
  proof app demonstrates every capability, not just `platform`/`storage`.
