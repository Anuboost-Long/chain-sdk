# Storage Capability (`desktop.storage`)

## How it works

Persistent local storage backed by a real SQLite file — one database per
app, opened lazily on first use in the OS's per-user app-data directory
(resolved via Tauri's `app_handle.path().app_data_dir()`). SQLite is
vendored via `rusqlite`'s `bundled` feature, so `crates/core/src/storage.rs`
is identical on every platform — no per-OS Rust code, unlike
Clipboard/Audio-style capabilities that need real native adapters per OS.

The public shape is deliberately close to the metal: `migrate(migrations)`
runs pending SQL migrations (tracked in an internal `_chain_migrations`
table, safe to call every app startup), `query(sql, params)` returns rows
as plain JSON objects, `execute(sql, params)` runs writes and returns
`{ rowsAffected, lastInsertId }`. No ORM, no typed models — the
consuming app (mneme) owns its own schema and query logic on top of this.

Requested by mneme as its first real capability gap beyond `platform` —
see `docs/chain-sdk-requests/01-local-storage.md` in the mneme repo for
the original ask and native-module survey that kicked this off.

## How to use it

```ts
import { desktop } from "@chain/sdk";

await desktop.storage.migrate([
  { version: 1, sql: "CREATE TABLE notes (id INTEGER PRIMARY KEY, title TEXT NOT NULL)" }
]);

const result = await desktop.storage.execute("INSERT INTO notes (title) VALUES (?1)", ["hello"]);
// result: { rowsAffected: 1, lastInsertId: 1 }

const rows = await desktop.storage.query<{ id: number; title: string }>(
  "SELECT id, title FROM notes"
);
```

Every already-scaffolded app gets this automatically via `chain update`
(it's a tracked file in `packages/cli/templates/lib.rs`) — no manual
wiring needed per app.

## Files to check

- `capabilities/storage/CONTRACT.md` — the semantic contract (API
  behavior, error model, explicit non-goals). Check this before changing
  behavior or adding a method.
- `capabilities/storage/contract.ts` — the exact types (`Migration`,
  `ExecuteResult`, `StorageApi`); change this and both implementations
  below together, never one without the others.
- `capabilities/storage/AGENTS.md` — the actual TODO checklist (Windows
  verification, contract tests, busy-timeout) and what's already been
  verified and how.
- `capabilities/storage/research/MACOS.md` /
  `capabilities/storage/research/WINDOWS.md` — platform-specific
  findings; Windows is explicitly **not yet verified** (no Windows
  machine was available) — read the checklist there before assuming it
  works.
- `crates/core/src/storage.rs` — the actual Rust implementation
  (`Database::open/migrate/query/execute`, JSON↔SQLite value conversion).
  Has a real unit test (`cargo test -p chain-core`) — extend it here
  rather than only testing through the Tauri layer.
- `packages/sdk/src/storage.ts` — SDK-side wrapper; handles the
  `isTauri()` check and wraps native errors into `ChainError`.
- `packages/cli/templates/lib.rs` — the Tauri command layer
  (`storage_migrate`/`storage_query`/`storage_execute`, lazy-open via
  `StorageState`) that every scaffolded app gets. This is the file
  `chain update` propagates — see `agent-docs/command/README.md`.
- `apps/playground/src-tauri/src/lib.rs` — same wiring, kept in sync by
  hand (playground isn't `chain init`-managed) so the framework's own
  proof app demonstrates every capability, not just `platform`.
