# Storage Capability — Agent Memory

Scope: `desktop.storage.migrate/query/execute/table/transaction` —
persistent SQLite-backed local storage. Requested by mneme (see
`docs/chain-sdk-requests/01-local-storage.md` in the mneme repo) as the
first real gap beyond `platform`: mneme's Course/Module/Page/Attachment
schema needs somewhere to live, and mneme's own rule is to never touch a
native filesystem/OS API directly.

Read order for a task in this capability:

1. Root `/AGENTS.md`
2. `/docs/ARCHITECTURE.md`
3. This file
4. `CONTRACT.md` + `contract.ts`
5. `research/MACOS.md` / `research/WINDOWS.md`
6. Relevant source in `crates/core/src/storage.rs` / `packages/sdk/src/storage.ts`

## What's already decided

- One SQLite database per app, opened lazily on first
  `migrate`/`query`/`execute` call — no separate "open" step in the
  public API (see CONTRACT.md).
- `rusqlite` with the `bundled` SQLite feature — same Rust code on every
  platform, no per-OS branching (SQLite is a portable C library; what
  differs per OS is only where Tauri resolves the app-data directory to,
  which Tauri already handles).
- Rows come back as plain JSON objects (column name → value). The app's
  `@Table` schema classes (read by the CLI, not at runtime) type them
  and generate its migrations — see CONTRACT.md and the command README's
  migrations section.
- Migrations are tracked in an internal `_chain_migrations` table this
  capability owns; re-running an already-applied migration is a no-op.

## Status

Implemented and verified for real on macOS:

- `crates/core/src/storage.rs` has a passing unit test
  (`migrate_query_execute_round_trip`) covering migrate → insert → query.
- Wired as real Tauri commands (`storage_migrate`/`storage_query`/
  `storage_execute`) in `packages/cli/templates/lib.rs` (propagates to
  every `chain init`/`chain update`'d app) and in `apps/playground`.
- Verified end to end in `mneme`'s actual running window: a temporary
  probe in `Home.tsx` ran `migrate` → `execute` (insert) → `query`
  through the real SDK → Tauri → Rust → SQLite path, confirmed via Rust
  stdout, including that rows **persisted across app restarts** (a real
  file-backed database, not an in-memory stand-in). The probe was
  reverted afterward — `Home.tsx`/`lib.rs` are back to the clean
  template.
- Propagated to `mneme` via `chain update` itself (not a fresh
  `chain init`) — real-world proof the update/merge feature works for a
  newly-added capability, not just a contrived test.

### Model-first migrations — 28 September 2026

- `storage.rs`: history gained `name`/`checksum` (upgraded in place);
  the runner turns `foreign_keys` off and `legacy_alter_table` on while
  migrating and restores both; a failing migration is rolled back. Unit
  tests: `records_name_and_checksum_and_upgrades_old_history_tables`,
  `failed_migration_rolls_back_and_restores_foreign_keys`,
  `rebuilds_a_table_that_another_tables_trigger_mentions`,
  `checksum_matches_fnv1a_reference_vectors`.
- **Real bug fixed:** mneme's hand-written migration 6 failed on any
  fresh database under SQLite ≥ 3.26 ("error in trigger
  agent_message_insert: no such table: main.agent_conversation"),
  reproduced with the app's own bundled SQLite 3.46 through
  `chain_core::storage::Database`. With `legacy_alter_table` on during
  migrations, all 15 apply.
- The generator itself lives in the CLI (`packages/cli/src/schema/`),
  tested by `packages/cli/test/schema.test.mjs`.

### Typed queries and transactions — 1 October 2026

mneme's request 26 (`docs/chain-sdk-requests/26-typed-queries.md` in the
mneme repo): 121 raw call sites with unchecked column strings,
hand-built partial UPDATEs, insert-then-reread, and non-atomic delete
cascades.

- `table<T>(name)` + `sql` tag: `packages/sdk/src/storage-table.ts`.
  Takes the `@Table` class as a type only — verified that mneme's Vite
  8 (oxc) build leaves TC39 decorators untransformed (a syntax error in
  the output) and minifies the class name, so a value import of a schema
  class can't work. Hence the explicit table name, and no runtime
  `@Column({ name })`/`@NotMapped()` awareness (see CONTRACT.md
  non-goals).
- `transaction()`: Rust `Database::begin/commit/rollback/
  rollback_abandoned`, a transaction id on `query`/`execute`; unit test
  `transaction_commits_or_rolls_back_and_refuses_outside_calls`, plus
  `insert_returning_reads_the_row_back_through_query` for `RETURNING *`.
- Builder SQL was run against real SQLite (`node:sqlite`) and the SDK
  queue/transaction logic against a fake `invoke` with the same
  begin/commit/refuse semantics: commit, rollback-and-rethrow, a plain
  call queued behind a transaction, and the 10 s `UNAVAILABLE` on a
  `desktop.storage` call inside the callback. Compile-time checks
  (unknown column, wrong value type, method as column, plain template
  string as a fragment) confirmed with `@ts-expect-error`.
- Propagated to mneme with `chain update` (only `.chain/native/src/lib.rs`
  changed), then verified in mneme's running `chain dev` window through
  `chain inspect --eval`, no probe edits to mneme's source: on a `TEMP`
  table, `insert` with a `sql` subquery value returned the stored row
  (defaults included), `update` by id with a `sql` value, `where` with
  `IN` + `orderBy desc`, `find`, a throwing transaction rolled back, a
  committing one returned its value, a plain read queued behind an open
  transaction ran after it, and an empty `delete` target was refused
  `INVALID_ARGUMENT`.

## What's NOT done yet (next steps for an agent to pick up)

- [ ] Verify on Windows — do NOT mark the contract/component status
      stable until confirmed there (rule: no single-platform contracts).
      See `research/WINDOWS.md` for the specific risks to check (WAL over
      network drives, antivirus file locking) and the verification
      checklist.
- [ ] Add contract tests under `capabilities/storage/tests/` — currently
      the only test is `crates/core/src/storage.rs`'s Rust unit test,
      which doesn't exercise the Tauri command layer or the SDK's
      `isTauri()`/error-mapping behavior.
- [ ] Configure an explicit SQLite busy-timeout (`PRAGMA busy_timeout`)
      — not set yet; relevant to the Windows antivirus-locking risk in
      `research/WINDOWS.md` and generally good practice for concurrent
      access.
- [ ] If an app needs `@Column({ name })` or `@NotMapped()` with
      `table()`, have the CLI generate a table descriptor (name, key,
      column map) from the classes it already reads, rather than making
      the decorators run.
- [ ] Update `docs/CAPABILITY_MATRIX.md`'s Windows column and
      `component.json`'s `platforms.windows` once Windows is verified.

## Rules specific to this capability

- Don't add app-level schema (Courses, Modules, ...) here — that's
  mneme's job, built on top of `migrate`/`query`/`execute`. This
  capability only knows how to run SQL, not what the SQL means.
- Don't grow `table()` into a full ORM (joins, relations, change
  tracking) speculatively; those queries stay raw SQL until an app's
  real need says otherwise.
- Don't add blob column support or multi-database support speculatively — each is a deliberate non-goal until a
  real app hits the need (see CONTRACT.md).
