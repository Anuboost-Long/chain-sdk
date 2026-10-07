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

## `desktop.storage.table<T>(name)`

```
table<T>(name: string): StorageTable<T>
```

A typed view of one table, EF Core `DbSet` style: `T` is the table's
`@Table` class, `name` its table name — `table<Course>("course")`.
Column names and value types are checked against `T` at compile time;
every non-method property of `T` is a column, and property names are
column names (the rule `query<T>` already follows).

The class goes in as a **type**, not a value: schema classes are read by
the CLI, not run, and app bundlers don't compile their decorators for
the webview (Vite 8 leaves TC39 decorators in the output, and minifies
class names), so importing one as a value would break the build. `name`
is therefore passed explicitly; a wrong one fails at runtime with
SQLite's "no such table".

Reads build up an immutable query — each call returns a new one:

```
where(filter: Filter<T> | SqlFragment)   // several where()s are ANDed
orderBy(...order: OrderBy<T>[])          // "position", "opened_at desc", or a fragment
limit(count: number)
all(): Promise<T[]>
first(): Promise<T | undefined>
```

A `Filter<T>` is an object of column equalities, ANDed: a value is
`= ?`, `null` is `IS NULL`, an array is `IN (?, …)` (an empty array
matches nothing), and `undefined` adds no condition — so optional
filters can be passed straight through: `where({ status, bookmarked })`.

Writes act on the table directly:

```
find(id): Promise<T | undefined>                 // by the `id` column
insert(values: Values<T>): Promise<T>            // the inserted row, as stored
update(target, values: Values<T>): Promise<T[]>  // only the given columns; the updated rows
delete(target): Promise<number>                  // rows deleted
```

`target` is an `id`, a `Filter<T>`, or a fragment. `update` and
`delete` reject with `INVALID_ARGUMENT` when the target has no
condition at all (e.g. `delete({ id: undefined })`), rather than
touching every row; pass ``sql`1` `` to really mean every row. In
`Values<T>`, `undefined` leaves the column out and a fragment writes an
SQL expression: `{ updated_at: sql`datetime('now')` }`. `insert` and
`update` read their rows back in the same statement (`RETURNING *`).

Joins, relation loading, aggregates, and change tracking are out — see
Non-goals. Those queries stay on `query`.

## `sql` — raw fragments

```
import { sql } from "@chain/sdk";

sql`date(created_at) >= date(${from})`
```

The escape hatch for what `Filter<T>` can't say (`date()`, `LIKE`,
subqueries, …), accepted by `where`, `orderBy`, `update`/`delete`
targets, and as a value in `Values<T>`. Every `${value}` becomes a bound
`?` parameter, never text in the SQL; an array becomes `(?, ?, …)` for
`IN`, and a nested fragment is spliced in with its own parameters.
Column names inside a fragment are not checked.

## `desktop.storage.transaction(work)`

```
transaction<R>(work: (tx: StorageScope) => Promise<R>): Promise<R>
```

Runs `work` in one SQLite transaction (`BEGIN IMMEDIATE`) and commits
when it resolves, or rolls back and rejects with its error when it
throws. Resolves with what `work` resolved with. `tx` has the same
`query`, `execute`, and `table` as `desktop.storage`, bound to the
transaction.

Storage calls run one at a time, in the order they're made. While a
transaction is open, other `desktop.storage` calls wait for it to
finish — so **inside `work`, use `tx`, never `desktop.storage`** (or a
nested `transaction()`): that call would wait for the transaction that
is waiting for it. A call that has waited 10 s on an open transaction
rejects with `UNAVAILABLE` instead of hanging.

A transaction left open by a page that reloaded or navigated away is
rolled back when the page starts loading. A call from another webview
while a transaction is open rejects with `NATIVE_FAILURE` rather than
joining it.

## Errors

Malformed SQL, constraint violations, and other database-level failures
reject with `ChainError { code: "NATIVE_FAILURE" }`, the underlying
SQLite message in `message`. Calling any method outside a Chain (Tauri)
runtime rejects with `ChainError { code: "UNSUPPORTED" }`, same as every
other capability.

## Non-goals

- `table()` is a typed query builder for single-table reads and
  writes, not a full ORM: no joins, no relation loading, no aggregates
  (`COUNT`, `GROUP BY`), no change tracking or unit of work. Those stay
  on `query`/`execute`. mneme asked for the query half on 1 October
  2026 (its request 26), which lifted the earlier "No ORM and no query
  builder" rule; the schema side has been `@Table` classes since 28
  September 2026.
- No property-to-column mapping at runtime: rows come back keyed by
  column name, so property names are column names. `@Column({ name })`
  and `@NotMapped()` are schema-only — `table()` doesn't know about
  them, since the classes never run (see `table()` above). Add a
  generated table descriptor from the CLI if an app needs either with
  `table()`.
- The app never migrates **down**; `down` is only run by
  `chain database update <earlier version>`.
- No multi-database support in this version — one database per app.
- No app-level table design (Courses, Modules, ...) — that's the
  consuming app's responsibility, built on top of `migrate`/`query`/
  `execute`.
- No nested transactions or savepoints.
