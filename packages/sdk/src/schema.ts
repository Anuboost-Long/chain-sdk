/**
 * Schema classes for `chain migration add` — see
 * agent-docs/framework/command/README.md's migrations section.
 *
 * Every decorator here is a marker and does nothing at runtime. The chain
 * CLI reads schema files with the TypeScript compiler instead, the way EF
 * reads C# types: a property's declared type becomes its SQLite type
 * (`string` TEXT, `number`/`boolean`/`bigint` INTEGER, `Uint8Array` BLOB),
 * `| null` or `?` makes it nullable, and every property of a `@Table` class
 * is a column unless marked `@NotMapped()`. Decorator arguments must
 * therefore be literals the CLI can read without running the code.
 *
 * The classes double as row types: `desktop.storage.query<Course>(...)`.
 */

type FieldDecorator = (value: undefined, context: ClassFieldDecoratorContext) => void;
type ClassDecorator = (value: abstract new (...args: never[]) => unknown, context: ClassDecoratorContext) => void;

const marker = () => {};

export type SqlType = "integer" | "text" | "real" | "blob" | "numeric";
export type ReferentialAction = "cascade" | "restrict" | "set null" | "set default" | "no action";

export interface TableOptions {
  /** Table-level CHECK constraints, as SQL expressions. */
  checks?: string[];
  /** Composite UNIQUE constraints, one column list each. */
  unique?: string[][];
}

/** Maps a class to a table. The name defaults to the class name in snake_case (`AiAction` → `ai_action`). */
export function Table(name?: string, options?: TableOptions): ClassDecorator {
  void name;
  void options;
  return marker;
}

export interface ColumnOptions {
  /** Column name, when it differs from the property name. */
  name?: string;
  /** Overrides the type inferred from the property, e.g. `"real"` for a fractional `number`. */
  type?: SqlType;
  /** A literal default: `@Column({ default: 0 })`, `@Column({ default: "lesson" })`. */
  default?: string | number | boolean | null;
  /** A SQL expression default: `@Column({ defaultSql: "datetime('now')" })`. */
  defaultSql?: string;
  unique?: boolean;
  /** A column CHECK constraint, as a SQL expression. */
  check?: string;
}

export function Column(options?: ColumnOptions): FieldDecorator {
  void options;
  return marker;
}

/** Marks the primary key. On several properties, they form a composite key. Without it, a property named `id` is the key. */
export function PrimaryKey(options?: { autoIncrement?: boolean }): FieldDecorator {
  void options;
  return marker;
}

export interface ForeignKeyOptions {
  /** Referenced column; defaults to the referenced table's primary key. */
  column?: string;
  onDelete?: ReferentialAction;
  onUpdate?: ReferentialAction;
}

export function ForeignKey(target: () => abstract new (...args: never[]) => unknown, options?: ForeignKeyOptions): FieldDecorator {
  void target;
  void options;
  return marker;
}

export interface IndexOptions {
  name?: string;
  unique?: boolean;
  /** Partial-index condition, as a SQL expression. */
  where?: string;
}

/** On a property: an index on that column. On a class: pass `columns` for a composite index. */
export function Index(options?: IndexOptions & { columns?: string[] }): FieldDecorator & ClassDecorator {
  void options;
  return marker;
}

/** A trigger that belongs to this class's table, as its full `CREATE TRIGGER` statement. */
export function Trigger(name: string, sql: string): ClassDecorator {
  void name;
  void sql;
  return marker;
}

/** Excludes a property from the table. */
export function NotMapped(): FieldDecorator {
  return marker;
}
