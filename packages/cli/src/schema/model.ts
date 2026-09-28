// The schema model every migration tool here works on: read from the app's
// `@Table` classes (read-classes.ts), read back from a real SQLite database
// (introspect.ts), saved per migration as `<version>-<name>.model.json`, and
// diffed into SQL (diff.ts). Values are normalized so a model read from
// classes and the same model read from a database compare equal.

export type SqlType = "INTEGER" | "TEXT" | "REAL" | "BLOB" | "NUMERIC";

export interface ColumnModel {
  name: string;
  type: SqlType;
  notNull: boolean;
  /** SQL text of the default, without surrounding parentheses: `0`, `'lesson'`, `datetime('now')`. */
  default?: string;
  unique?: boolean;
  check?: string;
}

export interface ForeignKeyModel {
  columns: string[];
  table: string;
  references: string[];
  onDelete?: string;
  onUpdate?: string;
}

export interface IndexModel {
  name: string;
  columns: string[];
  unique: boolean;
  where?: string;
}

export interface TriggerModel {
  name: string;
  sql: string;
}

export interface TableModel {
  name: string;
  columns: ColumnModel[];
  primaryKey?: { columns: string[]; autoIncrement: boolean };
  foreignKeys: ForeignKeyModel[];
  indexes: IndexModel[];
  uniques: string[][];
  checks: string[];
  triggers: TriggerModel[];
}

export interface SchemaModel {
  tables: TableModel[];
}

export function sqlTypeFor(declared: string): SqlType {
  const type = declared.toUpperCase();
  if (type.includes("INT")) return "INTEGER";
  if (type.includes("CHAR") || type.includes("CLOB") || type.includes("TEXT")) return "TEXT";
  if (type === "" || type.includes("BLOB")) return "BLOB";
  if (type.includes("REAL") || type.includes("FLOA") || type.includes("DOUB")) return "REAL";
  return "NUMERIC";
}

/** Collapses whitespace so formatting differences never count as a change. */
export function normalizeSql(sql: string): string {
  return sql.replace(/\s+/g, " ").replaceAll("( ", "(").replaceAll(" )", ")").trim();
}

/** Strips parentheses that wrap the whole expression: `(datetime('now'))` → `datetime('now')`. */
export function stripOuterParens(sql: string): string {
  let text = sql.trim();
  while (text.startsWith("(") && closingParen(text, 0) === text.length - 1) {
    text = text.slice(1, -1).trim();
  }
  return text;
}

/** Index of the parenthesis closing the one at `open`, skipping quoted text. */
export function closingParen(text: string, open: number): number {
  let depth = 0;
  let i = open;
  while (i < text.length && i !== -1) {
    const ch = text[i];
    if (ch === "'" || ch === '"' || ch === "`") {
      i = text.indexOf(ch, i + 1);
    } else if (ch === "[") {
      i = text.indexOf("]", i + 1);
    } else if (ch === "(") {
      depth++;
    } else if (ch === ")" && --depth === 0) {
      return i;
    }
    if (i !== -1) i++;
  }
  return -1;
}

export function normalizeAction(action: string | undefined): string | undefined {
  const value = action?.trim().toUpperCase();
  return !value || value === "NO ACTION" ? undefined : value;
}

/** A deterministic form of the model: tables, indexes, and triggers sorted by name, empty fields removed. */
export function canonicalize(model: SchemaModel): SchemaModel {
  const byName = <T extends { name: string }>(a: T, b: T) => a.name.localeCompare(b.name);
  return {
    tables: [...model.tables].sort(byName).map((table) => {
      const pkColumns = table.primaryKey?.columns ?? [];
      const singleIntegerKey =
        pkColumns.length === 1 && table.columns.find((c) => c.name === pkColumns[0])?.type === "INTEGER";
      return {
        name: table.name,
        columns: table.columns.map((column) => {
          const out: ColumnModel = {
            name: column.name,
            type: column.type,
            // SQLite reports an INTEGER PRIMARY KEY as nullable; it's a rowid alias and can't hold NULL anyway.
            notNull: singleIntegerKey && pkColumns[0] === column.name ? false : column.notNull
          };
          if (column.default !== undefined) out.default = stripOuterParens(column.default);
          if (column.unique) out.unique = true;
          if (column.check) out.check = normalizeSql(stripOuterParens(column.check));
          return out;
        }),
        ...(table.primaryKey ? { primaryKey: table.primaryKey } : {}),
        foreignKeys: [...table.foreignKeys]
          .map((fk) => ({
            columns: fk.columns,
            table: fk.table,
            references: fk.references,
            ...(normalizeAction(fk.onDelete) ? { onDelete: normalizeAction(fk.onDelete) } : {}),
            ...(normalizeAction(fk.onUpdate) ? { onUpdate: normalizeAction(fk.onUpdate) } : {})
          }))
          .sort((a, b) => a.columns.join().localeCompare(b.columns.join())),
        indexes: [...table.indexes].sort(byName).map((index) => ({
          name: index.name,
          columns: index.columns,
          unique: index.unique,
          ...(index.where ? { where: normalizeSql(stripOuterParens(index.where)) } : {})
        })),
        uniques: [...table.uniques].sort((a, b) => a.join().localeCompare(b.join())),
        checks: table.checks.map((check) => normalizeSql(stripOuterParens(check))).sort((a, b) => a.localeCompare(b)),
        triggers: [...table.triggers].sort(byName).map((trigger) => ({ name: trigger.name, sql: trigger.sql.trim() }))
      };
    })
  };
}

/** Structural equality, ignoring formatting inside trigger bodies. */
export function sameTable(a: TableModel, b: TableModel): boolean {
  return JSON.stringify(comparable(a)) === JSON.stringify(comparable(b));
}

export function comparable<T>(value: T): T {
  return JSON.parse(
    JSON.stringify(value, (key, v) => (key === "sql" && typeof v === "string" ? normalizeSql(v) : v))
  ) as T;
}

export function modelsEqual(a: SchemaModel, b: SchemaModel): boolean {
  return JSON.stringify(comparable(canonicalize(a))) === JSON.stringify(comparable(canonicalize(b)));
}

/** Double-quoted SQL identifier. */
export function quote(identifier: string): string {
  return `"${identifier.replaceAll('"', '""')}"`;
}

export function snakeCase(name: string): string {
  return name
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/([A-Z])([A-Z][a-z])/g, "$1_$2")
    .toLowerCase();
}

export function pascalCase(name: string): string {
  return name
    .split(/[^a-zA-Z0-9]+/)
    .filter(Boolean)
    .map((part) => part[0].toUpperCase() + part.slice(1))
    .join("");
}
