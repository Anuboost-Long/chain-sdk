// Turns the difference between two schema models into SQLite SQL, the way
// `dotnet ef migrations add` turns a model diff into Up/Down operations.
// Down is the same diff run in reverse, so every migration can be undone.
//
// SQLite only alters a few things in place (ADD/RENAME/DROP COLUMN, RENAME
// TABLE). Everything else rebuilds the table with the 12-step procedure from
// https://www.sqlite.org/lang_altertable.html: create the new shape, copy the
// rows, drop the old table, rename, then recreate its indexes and triggers.
// The runner (storage.rs / history.ts) turns foreign-key enforcement off
// while migrating, so the DROP can't cascade into child tables.

import {
  canonicalize,
  quote,
  sameTable,
  type ColumnModel,
  type IndexModel,
  type SchemaModel,
  type TableModel
} from "./model.js";

export interface Renames {
  /** old table name → new table name */
  tables: Map<string, string>;
  /** new table name → (new column name → old column name) */
  columns: Map<string, Map<string, string>>;
}

export const noRenames = (): Renames => ({ tables: new Map(), columns: new Map() });

export interface MigrationPlan {
  up: string;
  down: string;
  /** Things that can lose or change data, for the developer to review. */
  warnings: string[];
  empty: boolean;
}

const literalPatterns = [/^-?\d+(?:\.\d+)?$/, /^'(?:[^']|'')*'$/, /^(?:NULL|TRUE|FALSE)$/i, /^X'[0-9A-F]*'$/i];

/** A constant ADD COLUMN accepts as a default; anything else needs a rebuild. */
function isLiteral(value: string): boolean {
  return literalPatterns.some((pattern) => pattern.test(value));
}

export function defaultSql(value: string): string {
  return isLiteral(value) || /^CURRENT_(?:TIME|DATE|TIMESTAMP)$/i.test(value) ? value : `(${value})`;
}

/** What existing rows get for a new NOT NULL column that has no default — EF's CLR-default behavior. */
function fillValue(column: ColumnModel): string {
  if (column.default !== undefined) return defaultSql(column.default);
  switch (column.type) {
    case "TEXT":
      return "''";
    case "BLOB":
      return "X''";
    default:
      return "0";
  }
}

function columnSql(table: TableModel, column: ColumnModel): string {
  const parts = [quote(column.name), column.type];
  const pk = table.primaryKey;
  const inlineKey = pk?.columns.length === 1 && pk.columns[0] === column.name;
  if (inlineKey) parts.push(pk.autoIncrement ? "PRIMARY KEY AUTOINCREMENT" : "PRIMARY KEY");
  if (column.notNull && !(inlineKey && column.type === "INTEGER")) parts.push("NOT NULL");
  if (column.unique) parts.push("UNIQUE");
  if (column.default !== undefined) parts.push(`DEFAULT ${defaultSql(column.default)}`);
  if (column.check) parts.push(`CHECK (${column.check})`);
  const fk = table.foreignKeys.find((f) => f.columns.length === 1 && f.columns[0] === column.name);
  if (fk) parts.push(referencesSql(fk.table, fk.references, fk.onDelete, fk.onUpdate));
  return parts.join(" ");
}

function referencesSql(table: string, columns: string[], onDelete?: string, onUpdate?: string): string {
  let sql = `REFERENCES ${quote(table)} (${columns.map(quote).join(", ")})`;
  if (onDelete) sql += ` ON DELETE ${onDelete}`;
  if (onUpdate) sql += ` ON UPDATE ${onUpdate}`;
  return sql;
}

export function createTableSql(table: TableModel, name = table.name): string {
  const lines = table.columns.map((column) => columnSql(table, column));
  if (table.primaryKey && table.primaryKey.columns.length > 1) {
    lines.push(`PRIMARY KEY (${table.primaryKey.columns.map(quote).join(", ")})`);
  }
  for (const fk of table.foreignKeys.filter((f) => f.columns.length > 1)) {
    lines.push(`FOREIGN KEY (${fk.columns.map(quote).join(", ")}) ${referencesSql(fk.table, fk.references, fk.onDelete, fk.onUpdate)}`);
  }
  for (const unique of table.uniques) lines.push(`UNIQUE (${unique.map(quote).join(", ")})`);
  for (const check of table.checks) lines.push(`CHECK (${check})`);
  return `CREATE TABLE ${quote(name)} (\n  ${lines.join(",\n  ")}\n);`;
}

export function createIndexSql(table: string, index: IndexModel): string {
  const where = index.where ? ` WHERE ${index.where}` : "";
  return `CREATE ${index.unique ? "UNIQUE " : ""}INDEX ${quote(index.name)} ON ${quote(table)} (${index.columns.map(quote).join(", ")})${where};`;
}

function triggerSql(sql: string): string {
  return sql.trim().endsWith(";") ? sql.trim() : `${sql.trim()};`;
}

/** Table-level shape that ALTER TABLE can't change in place. */
function constraintsChanged(from: TableModel, to: TableModel, renameOf: (column: string) => string): boolean {
  const renamed = (columns: string[]) => columns.map(renameOf);
  const shape = (t: TableModel, map: (c: string[]) => string[]) =>
    JSON.stringify({
      primaryKey: t.primaryKey ? { ...t.primaryKey, columns: map(t.primaryKey.columns) } : undefined,
      foreignKeys: t.foreignKeys.map((fk) => ({ ...fk, columns: map(fk.columns) })),
      uniques: t.uniques.map(map),
      checks: t.checks
    });
  return shape(from, renamed) !== shape(to, (c) => c);
}

function columnChanged(from: ColumnModel, to: ColumnModel): boolean {
  return (
    from.type !== to.type ||
    from.notNull !== to.notNull ||
    from.default !== to.default ||
    Boolean(from.unique) !== Boolean(to.unique) ||
    from.check !== to.check
  );
}

function tableSql(from: TableModel, to: TableModel, columnRenames: Map<string, string>, warnings: string[]): string[] {
  const sql: string[] = [];
  const oldName = (column: string) => columnRenames.get(column) ?? column;
  const renameOf = (column: string) => [...columnRenames].find(([, old]) => old === column)?.[0] ?? column;
  const fromColumns = new Map(from.columns.map((c) => [c.name, c]));
  const renamedFrom = new Set(columnRenames.values());

  const added = to.columns.filter((c) => !fromColumns.has(oldName(c.name)));
  const dropped = from.columns.filter((c) => !to.columns.some((t) => oldName(t.name) === c.name) && !renamedFrom.has(c.name));
  const kept = to.columns.filter((c) => fromColumns.has(oldName(c.name)));

  const inIndex = (t: TableModel, column: string) => t.indexes.some((i) => i.columns.includes(column));
  const mentioned = (t: TableModel, column: string) =>
    [...t.checks, ...t.triggers.map((tr) => tr.sql), ...t.columns.map((c) => c.check ?? "")].some((text) =>
      new RegExp(String.raw`\b${column}\b`).test(text)
    );
  const addable = (c: ColumnModel) =>
    !c.unique &&
    !to.primaryKey?.columns.includes(c.name) &&
    !to.foreignKeys.some((fk) => fk.columns.includes(c.name)) &&
    (c.default === undefined ? !c.notNull : isLiteral(c.default));
  const droppable = (c: ColumnModel) =>
    !c.unique &&
    !from.primaryKey?.columns.includes(c.name) &&
    !from.foreignKeys.some((fk) => fk.columns.includes(c.name)) &&
    !inIndex(from, c.name) &&
    !mentioned(from, c.name);

  const rebuild =
    constraintsChanged(from, to, renameOf) ||
    kept.some((c) => columnChanged(fromColumns.get(oldName(c.name))!, c)) ||
    added.some((c) => !addable(c)) ||
    dropped.some((c) => !droppable(c));

  for (const c of dropped) warnings.push(`Drops column ${from.name}.${c.name} and its data.`);
  for (const c of added.filter((c) => c.notNull && c.default === undefined)) {
    warnings.push(`New NOT NULL column ${to.name}.${c.name} has no default; existing rows get ${fillValue(c)}.`);
  }
  for (const c of kept) {
    const before = fromColumns.get(oldName(c.name))!;
    if (!before.notNull && c.notNull) warnings.push(`${to.name}.${c.name} becomes NOT NULL; existing NULLs become ${fillValue(c)}.`);
    if (before.type !== c.type) warnings.push(`${to.name}.${c.name} changes type ${before.type} → ${c.type}; values are copied as they are.`);
  }

  if (rebuild) {
    const temp = `__chain_new_${to.name}`;
    const copied = [
      ...kept.map((c) => {
        const source = quote(oldName(c.name));
        const before = fromColumns.get(oldName(c.name))!;
        return { target: c.name, value: !before.notNull && c.notNull ? `COALESCE(${source}, ${fillValue(c)})` : source };
      }),
      ...added.filter((c) => c.notNull && c.default === undefined).map((c) => ({ target: c.name, value: fillValue(c) }))
    ];
    const changes = [
      ...added.map((c) => `add ${c.name}`),
      ...dropped.map((c) => `drop ${c.name}`),
      ...[...columnRenames].map(([renamed, old]) => `rename ${old} → ${renamed}`),
      ...kept.filter((c) => columnChanged(fromColumns.get(oldName(c.name))!, c)).map((c) => `change ${c.name}`),
      ...(constraintsChanged(from, to, renameOf) ? ["change keys or constraints"] : [])
    ];
    sql.push(
      `-- Rebuild ${quote(to.name)} (${changes.join(", ")}): SQLite can't do this with ALTER TABLE.`,
      createTableSql(to, temp),
      `INSERT INTO ${quote(temp)} (${copied.map((c) => quote(c.target)).join(", ")})\n  SELECT ${copied.map((c) => c.value).join(", ")} FROM ${quote(from.name)};`,
      `DROP TABLE ${quote(from.name)};`,
      // Legacy mode keeps SQLite from re-validating other tables' triggers
      // that mention this table while it's momentarily gone.
      "PRAGMA legacy_alter_table = ON;",
      `ALTER TABLE ${quote(temp)} RENAME TO ${quote(to.name)};`,
      "PRAGMA legacy_alter_table = OFF;",
      ...to.indexes.map((index) => createIndexSql(to.name, index)),
      ...to.triggers.map((trigger) => triggerSql(trigger.sql))
    );
    return sql;
  }

  const fromIndexes = new Map(from.indexes.map((i) => [i.name, i]));
  const toIndexes = new Map(to.indexes.map((i) => [i.name, i]));
  const indexKey = (i: IndexModel, map: (c: string) => string) => JSON.stringify({ ...i, columns: i.columns.map(map) });
  const staleIndexes = from.indexes.filter(
    (i) => !toIndexes.has(i.name) || indexKey(i, renameOf) !== indexKey(toIndexes.get(i.name)!, (c) => c)
  );
  const fromTriggers = new Map(from.triggers.map((t) => [t.name, t]));
  const toTriggers = new Map(to.triggers.map((t) => [t.name, t]));
  const same = (a?: string, b?: string) => a?.replace(/\s+/g, " ").trim() === b?.replace(/\s+/g, " ").trim();
  const staleTriggers = from.triggers.filter((t) => !same(t.sql, toTriggers.get(t.name)?.sql));

  const staleNames = new Set(staleIndexes.map((i) => i.name));
  sql.push(
    ...staleTriggers.map((t) => `DROP TRIGGER ${quote(t.name)};`),
    ...staleIndexes.map((i) => `DROP INDEX ${quote(i.name)};`),
    ...[...columnRenames].map(
      ([newName, old]) => `ALTER TABLE ${quote(to.name)} RENAME COLUMN ${quote(old)} TO ${quote(newName)};`
    ),
    ...dropped.map((c) => `ALTER TABLE ${quote(to.name)} DROP COLUMN ${quote(c.name)};`),
    ...added.map((c) => `ALTER TABLE ${quote(to.name)} ADD COLUMN ${columnSql(to, c)};`),
    ...to.indexes.filter((i) => !fromIndexes.has(i.name) || staleNames.has(i.name)).map((i) => createIndexSql(to.name, i)),
    ...to.triggers.filter((t) => !same(t.sql, fromTriggers.get(t.name)?.sql)).map((t) => triggerSql(t.sql))
  );
  return sql;
}

function diffSql(fromModel: SchemaModel, toModel: SchemaModel, renames: Renames, warnings: string[]): string[] {
  const from = canonicalize(fromModel);
  const to = canonicalize(toModel);
  const sql: string[] = [];
  const toTables = new Map(to.tables.map((t) => [t.name, t]));
  const renamedTo = new Set(renames.tables.values());

  for (const table of from.tables) {
    const newName = renames.tables.get(table.name);
    if (newName) sql.push(`ALTER TABLE ${quote(table.name)} RENAME TO ${quote(newName)};`);
    else if (!toTables.has(table.name)) {
      warnings.push(`Drops table ${table.name} and its data.`);
      sql.push(`DROP TABLE ${quote(table.name)};`);
    }
  }
  for (const table of to.tables) {
    const oldName = [...renames.tables].find(([, n]) => n === table.name)?.[0] ?? table.name;
    const before = from.tables.find((t) => t.name === oldName);
    if (!before || (!renamedTo.has(table.name) && before.name !== table.name)) {
      sql.push(createTableSql(table), ...table.indexes.map((i) => createIndexSql(table.name, i)), ...table.triggers.map((t) => triggerSql(t.sql)));
      continue;
    }
    const columnRenames = renames.columns.get(table.name) ?? new Map<string, string>();
    const current = { ...before, name: table.name };
    if (columnRenames.size === 0 && sameTable(current, table)) continue;
    sql.push(...tableSql(current, table, columnRenames, warnings));
  }
  return sql;
}

function invert(renames: Renames): Renames {
  const tables = new Map([...renames.tables].map(([old, renamed]) => [renamed, old]));
  const columns = new Map<string, Map<string, string>>();
  for (const [table, map] of renames.columns) {
    const oldTable = [...renames.tables].find(([, n]) => n === table)?.[0] ?? table;
    columns.set(oldTable, new Map([...map].map(([renamed, old]) => [old, renamed])));
  }
  return { tables, columns };
}

function wrap(statements: string[]): string {
  return statements.length ? `BEGIN;\n\n${statements.join("\n\n")}\n\nCOMMIT;` : "";
}

export function planMigration(from: SchemaModel, to: SchemaModel, renames: Renames = noRenames()): MigrationPlan {
  const warnings: string[] = [];
  const up = diffSql(from, to, renames, warnings);
  const down = diffSql(to, from, invert(renames), []);
  return { up: wrap(up), down: wrap(down), warnings, empty: up.length === 0 };
}
