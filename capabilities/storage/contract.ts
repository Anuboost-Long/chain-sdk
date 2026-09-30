/**
 * Structural contract for the Storage capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface Migration {
  version: number;
  /** Applied going up. */
  sql: string;
  /** Recorded in the migration history. Generated migrations always have one. */
  name?: string;
  /** Reverts `sql`. Only `chain database update <target>` runs it; the app never migrates down. */
  down?: string;
}

export interface ExecuteResult {
  rowsAffected: number;
  lastInsertId: number;
}

/** Raw SQL with its values bound as parameters. Build one with the `sql` tag, never by hand. */
export interface SqlFragment {
  readonly sql: string;
  readonly params: readonly unknown[];
}

/** A tagged template: each `${value}` becomes a bound `?`, an array becomes `(?, ?, …)`. */
export type SqlTag = (strings: TemplateStringsArray, ...values: unknown[]) => SqlFragment;

/** The properties of a `@Table` class that are columns: every one that isn't a method. */
export type Column<T> = {
  [K in keyof T]-?: T[K] extends (...args: never[]) => unknown ? never : K;
}[keyof T] &
  string;

/** Column equalities, ANDed: a value is `=`, `null` is `IS NULL`, an array is `IN`, `undefined` is no condition. */
export type Filter<T> = { [K in Column<T>]?: T[K] | readonly T[K][] };

/** Columns to write; `undefined` leaves one out, a fragment writes an SQL expression. */
export type Values<T> = { [K in Column<T>]?: T[K] | SqlFragment };

export type OrderBy<T> = Column<T> | `${Column<T>} desc` | SqlFragment;

/** The type of `T`'s `id` column — what `find`, `update` and `delete` take as a key. */
export type Id<T> = T extends { id: infer I } ? I : never;

/** Which rows `update` and `delete` act on: an `id`, a filter, or a fragment. */
export type Target<T> = Id<T> | Filter<T> | SqlFragment;

export interface TableQuery<T> {
  where(filter: Filter<T> | SqlFragment): TableQuery<T>;
  orderBy(...order: OrderBy<T>[]): TableQuery<T>;
  limit(count: number): TableQuery<T>;
  all(): Promise<T[]>;
  first(): Promise<T | undefined>;
}

export interface StorageTable<T> extends TableQuery<T> {
  find(id: Id<T>): Promise<T | undefined>;
  /** Resolves with the inserted row, as stored. */
  insert(values: Values<T>): Promise<T>;
  /** Resolves with the updated rows. */
  update(target: Target<T>, values: Values<T>): Promise<T[]>;
  /** Resolves with the number of rows deleted. */
  delete(target: Target<T>): Promise<number>;
}

/** What `desktop.storage` and a transaction's `tx` both offer. */
export interface StorageScope {
  query<T = unknown>(sql: string, params?: unknown[]): Promise<T[]>;
  execute(sql: string, params?: unknown[]): Promise<ExecuteResult>;
  /** `T` is the table's `@Table` class, `name` its table name: `table<Course>("course")`. */
  table<T>(name: string): StorageTable<T>;
}

export interface StorageApi extends StorageScope {
  migrate(migrations: Migration[]): Promise<void>;
  /** Commits when `work` resolves, rolls back when it throws. Inside `work`, use `tx`, not `desktop.storage`. */
  transaction<R>(work: (tx: StorageScope) => Promise<R>): Promise<R>;
}
