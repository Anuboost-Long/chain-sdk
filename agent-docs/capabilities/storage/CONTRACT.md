# Storage Capability — Contract

## What this is

Persistent, structured local storage for an app's own data — a single
SQLite-backed database per app, stored in the OS's per-user app-data
directory. Backs things like mneme's Course/Module/Page/Attachment
tables; the schema itself is app-level and not part of this capability
(see Non-goals).

## `desktop.storage.migrate(migrations)`

```
migrate(migrations: Migration[]): Promise<void>

interface Migration {
  version: number;  // must be unique and increasing across the app's lifetime
  sql: string;       // one or more statements, run as a single batch
  name?: string;     // recorded in the history; generated migrations always have one
  down?: string;     // reverts `sql`; ignored here — only the CLI migrates down
}
```

Runs every migration whose `version` hasn't been applied yet, in
ascending `version` order, each as one batch. Applied versions are
tracked in an internal `_chain_migrations` table the capability owns —
don't create a table with that name. Calling `migrate()` again with the
same (or a prefix of the same) migrations is a no-op for anything already
applied — safe to call on every app startup.

The database is created, and the data directory created if missing, the
first time any `desktop.storage.*` call is made — no separate "open"
step in the public API.

`_chain_migrations` records each migration's `version`, `applied_at`,
`name`, and `checksum` (FNV-1a 64 of `sql`, hex), so the CLI can show the
history and flag a migration edited after it ran. Older history tables
gain the two columns in place.

While migrating, foreign-key enforcement is off and
`legacy_alter_table` is on — both needed by SQLite's table-rebuild
procedure — and both are restored afterwards. A migration that fails
part-way is rolled back; ones before it stay applied, and `migrate()`
rejects with the failure.

Migrations are normally **generated**, not written by hand: the app
declares its schema as `@Table` classes (`@chain/sdk/schema`) and
`chain migration add <name>` diffs them into a migration with both `sql`
and `down` — see `agent-docs/framework/command/README.md`. That's CLI
tooling around this API; `migrate()` itself runs whatever it's given.

## `desktop.storage.query(sql, params?)`

```
query<T = unknown>(sql: string, params?: unknown[]): Promise<T[]>
```

Runs a parametrized read (`SELECT`) and resolves with one plain object
per row, column name to value. Use `?` placeholders in `sql`, positional
values in `params` — never string-interpolate values into `sql`.

## `desktop.storage.execute(sql, params?)`

```
execute(sql: string, params?: unknown[]): Promise<ExecuteResult>

interface ExecuteResult {
  rowsAffected: number;
  lastInsertId: number;
}
```

Runs a parametrized write (`INSERT`/`UPDATE`/`DELETE`/DDL). Same
parameter-binding rule as `query`.

## Errors

Malformed SQL, constraint violations, and other database-level failures
reject with `ChainError { code: "NATIVE_FAILURE" }`, the underlying
SQLite message in `message`. Calling any method outside a Chain (Tauri)
runtime rejects with `ChainError { code: "UNSUPPORTED" }`, same as every
other capability.

## Non-goals

- No ORM and no query builder: `query`/`execute` take raw SQL. The
  schema, though, is declared as `@Table` classes that migrations are
  generated from, and those classes are the row types
  (`query<Course>(…)`) — the user asked for `dotnet ef`-style
  model-first migrations explicitly (28 September 2026), which replaced
  the earlier "no generated models" rule. Query mapping stays out: rows
  come back keyed by column name, so property names are column names.
- The app never migrates **down**; `down` is only run by
  `chain database update <earlier version>`.
- No multi-database support in this version — one database per app.
- No app-level table design (Courses, Modules, ...) — that's the
  consuming app's responsibility, built on top of `migrate`/`query`/
  `execute`.
- No cross-call transaction API yet — add one (e.g. a `transaction()`
  wrapper) only when a real app needs multi-statement atomicity that
  `execute`'s single-statement-per-call model can't express.
