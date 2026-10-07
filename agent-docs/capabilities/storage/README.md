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
`{ rowsAffected, lastInsertId }`. The **schema** is declared as `@Table`
classes (`@chain/sdk/schema`), and `chain migration add <name>` generates
each migration (Up and Down SQL) from changes to them, EF Core style —
see `agent-docs/framework/command/README.md`.

`table<Course>("course")` is the query half, EF `DbSet` style: a typed
single-table builder (`where`/`orderBy`/`limit`/`all`/`first`, plus
`find`/`insert`/`update`/`delete`) whose columns are checked against the
class at compile time. It's pure TypeScript in
`packages/sdk/src/storage-table.ts`, compiling to parametrized SQL on top
of `query`/`execute` — `insert`/`update` use `RETURNING *` to hand the
row back in one statement. The class is passed as a type only: Vite 8
leaves TC39 decorators uncompiled and minifies class names, so schema
classes can't be imported as values in the webview. The `sql` tagged
template is the raw-fragment escape hatch, always bound.

`transaction(work)` is native: `storage_begin` returns an id, calls
inside carry it (`storage_query`/`storage_execute`'s `transaction`
arg), `storage_commit`/`storage_rollback` close it. Rust refuses a call
without the open transaction's id instead of letting it join. The SDK
runs every storage call through one queue (`inLine` in `storage.ts`), so
calls from the same page wait for an open transaction rather than being
refused; one that waits 10 s rejects `UNAVAILABLE` (the "used
`desktop.storage` inside the callback" deadlock). An `on_page_load`
hook in `lib.rs` rolls back a transaction abandoned by a reload.

Requested by mneme as its first real capability gap beyond `platform` —
see `docs/chain-sdk-requests/01-local-storage.md` in the mneme repo for
the original ask and native-module survey that kicked this off.

## How to use it

Declare the schema, then let the CLI write the migration:

```ts
// src/lib/db/schema/note.ts
import { PrimaryKey, Table } from "@chain/sdk/schema";

@Table()
export class Note {
  @PrimaryKey({ autoIncrement: true }) id!: number;
  title!: string;
}
```

```bash
chain migration add create-notes   # writes db/migrations/0001-create-notes.ts (+ .model.json)
```

```ts
import { desktop } from "@chain/sdk";
import { migrations } from "./lib/db/migrations";
import type { Note } from "./lib/db/schema";

await desktop.storage.migrate(migrations); // on startup; applies what hasn't run

const result = await desktop.storage.execute("INSERT INTO note (title) VALUES (?1)", ["hello"]);
// result: { rowsAffected: 1, lastInsertId: 1 }

const rows = await desktop.storage.query<Note>("SELECT id, title FROM note");

// Or typed, through the class:
const notes = desktop.storage.table<Note>("note");
const note = await notes.insert({ title: "hello" });             // the stored row
await notes.update(note.id, { title: sql`upper(title)` });       // import { sql } from "@chain/sdk"
const recent = await notes.where({ id: [1, 2] }).orderBy("id desc").all();

await desktop.storage.transaction(async (tx) => {
  await tx.table<Note>("note").delete({ id: note.id });            // tx, never desktop.storage, in here
});
```

Every already-scaffolded app gets this automatically via `chain update`
(it's a tracked file in `packages/cli/templates/lib.rs`) — no manual
wiring needed per app.

## Files to check

- `agent-docs/capabilities/storage/CONTRACT.md` — the semantic contract
  (API behavior, error model, explicit non-goals). Check this before
  changing behavior or adding a method.
- `capabilities/storage/contract.ts` — the exact types (`Migration`,
  `ExecuteResult`, `StorageApi`); change this and both implementations
  below together, never one without the others.
- `agent-docs/capabilities/storage/AGENTS.md` — the actual TODO checklist
  (Windows verification, contract tests, busy-timeout) and what's already
  been verified and how.
- `agent-docs/capabilities/storage/research/MACOS.md` /
  `agent-docs/capabilities/storage/research/WINDOWS.md` — platform-specific
  findings; Windows is explicitly **not yet verified** (no Windows
  machine was available) — read the checklist there before assuming it
  works.
- `crates/core/src/storage.rs` — the actual Rust implementation
  (`Database::open/migrate/query/execute`, `begin/commit/rollback`,
  JSON↔SQLite value conversion).
- `packages/sdk/src/storage.ts` — the SDK wrapper: the call queue and
  `transaction()`.
- `packages/sdk/src/storage-table.ts` — `table()`'s SQL builder and the
  `sql` tag. Pure TypeScript; nothing native.
- `packages/cli/templates/lib.rs` — the `storage_*` Tauri commands and
  the `on_page_load` rollback hook (mirrored in `apps/playground`).
  Has a real unit test (`cargo test -p chain-core`) — extend it here
  rather than only testing through the Tauri layer.
- `packages/sdk/src/storage.ts` — SDK-side wrapper; handles the
  `isTauri()` check and wraps native errors into `ChainError`.
- `packages/cli/templates/lib.rs` — the Tauri command layer
  (`storage_migrate`/`storage_query`/`storage_execute`, lazy-open via
  `StorageState`) that every scaffolded app gets. This is the file
  `chain update` propagates — see `agent-docs/framework/command/README.md`.
- `apps/playground/src-tauri/src/lib.rs` — same wiring, kept in sync by
  hand (playground isn't `chain init`-managed) so the framework's own
  proof app demonstrates every capability, not just `platform`.
