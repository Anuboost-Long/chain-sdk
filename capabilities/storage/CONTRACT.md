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

- No ORM, no generated typed models, no query builder. `query`/`execute`
  take raw SQL — the app owns its schema and query logic.
- No multi-database support in this version — one database per app.
- No app-level table design (Courses, Modules, ...) — that's the
  consuming app's responsibility, built on top of `migrate`/`query`/
  `execute`.
- No cross-call transaction API yet — add one (e.g. a `transaction()`
  wrapper) only when a real app needs multi-statement atomicity that
  `execute`'s single-statement-per-call model can't express.
