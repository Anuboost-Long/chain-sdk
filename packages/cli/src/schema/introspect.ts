// Reads a schema model back out of a real SQLite database: PRAGMAs for
// columns, keys, and indexes, plus the stored CREATE statements for what
// PRAGMAs don't report (AUTOINCREMENT, CHECK, UNIQUE, partial-index WHERE).
// Used by `chain database scaffold` and to verify generated migrations.

import type { DatabaseSync } from "node:sqlite";

import {
  closingParen,
  sqlTypeFor,
  type ColumnModel,
  type ForeignKeyModel,
  type IndexModel,
  type SchemaModel,
  type TableModel
} from "./model.js";

interface MasterRow {
  type: string;
  name: string;
  tbl_name: string;
  sql: string | null;
}

interface ColumnRow {
  name: string;
  type: string | null;
  notnull: number;
  dflt_value: string | null;
  pk: number;
  hidden: number;
}

interface ForeignKeyRow {
  id: number;
  seq: number;
  table: string;
  from: string;
  to: string | null;
  on_update: string;
  on_delete: string;
}

interface IndexListRow {
  name: string;
  unique: number;
  origin: string;
}

interface IndexInfoRow {
  name: string | null;
}

function rows<T>(db: DatabaseSync, sql: string, ...params: string[]): T[] {
  return db.prepare(sql).all(...params) as unknown as T[];
}

/** Splits `a, b (c, d), 'e,f'` on top-level commas. */
function splitTopLevel(text: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  let quoteChar = "";
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (quoteChar) {
      if (ch === quoteChar) quoteChar = "";
    } else if (ch === "'" || ch === '"' || ch === "`") {
      quoteChar = ch;
    } else if (ch === "[") {
      quoteChar = "]";
    } else if (ch === "(") {
      depth++;
    } else if (ch === ")") {
      depth--;
    } else if (ch === "," && depth === 0) {
      parts.push(text.slice(start, i).trim());
      start = i + 1;
    }
  }
  parts.push(text.slice(start).trim());
  return parts.filter(Boolean);
}

/** The text with quoted strings and parenthesized groups blanked, for keyword searches. */
function blankNested(text: string): string {
  let out = "";
  let depth = 0;
  let quoteChar = "";
  for (const ch of text) {
    if (quoteChar) {
      if (ch === quoteChar) quoteChar = "";
      out += " ";
    } else if (ch === "'" || ch === '"' || ch === "`") {
      quoteChar = ch;
      out += " ";
    } else if (ch === "(") {
      depth++;
      out += " ";
    } else if (ch === ")") {
      depth--;
      out += " ";
    } else {
      out += depth > 0 ? " " : ch;
    }
  }
  return out;
}

/** Every `CHECK (...)` expression in a definition. */
function checksIn(definition: string): string[] {
  const checks: string[] = [];
  const blanked = blankNested(definition);
  const pattern = /\bCHECK\s*$/i;
  let i = 0;
  while (i < definition.length) {
    const end = definition[i] === "(" && pattern.test(blanked.slice(0, i)) ? closingParen(definition, i) : -1;
    if (end > i) {
      checks.push(definition.slice(i + 1, end).trim());
      i = end; // parentheses inside the expression, like IN (...), aren't new checks
    }
    i++;
  }
  return checks;
}

function unquote(identifier: string): string {
  const text = identifier.trim();
  const first = text[0];
  if (first === '"' || first === "`" || first === "[") return text.slice(1, -1).replaceAll('""', '"');
  return text;
}

function identifierList(text: string): string[] {
  return splitTopLevel(text).map((part) => unquote(part.split(/\s+/)[0]));
}

interface ParsedCreate {
  autoIncrement: boolean;
  columnChecks: Map<string, string>;
  columnUnique: Set<string>;
  checks: string[];
  uniques: string[][];
}

function parseCreateTable(sql: string): ParsedCreate {
  const open = sql.indexOf("(");
  const body = sql.slice(open + 1, closingParen(sql, open));
  const parsed: ParsedCreate = {
    autoIncrement: /\bAUTOINCREMENT\b/i.test(blankNested(body)),
    columnChecks: new Map(),
    columnUnique: new Set(),
    checks: [],
    uniques: []
  };
  for (const part of splitTopLevel(body)) {
    const definition = part.replace(/^CONSTRAINT\s+("[^"]*"|\S+)\s+/i, "");
    const keyword = definition.split(/[\s(]/)[0].toUpperCase();
    if (keyword === "CHECK") {
      parsed.checks.push(...checksIn(definition));
    } else if (keyword === "UNIQUE") {
      const open = definition.indexOf("(");
      parsed.uniques.push(identifierList(definition.slice(open + 1, closingParen(definition, open))));
    } else if (keyword !== "PRIMARY" && keyword !== "FOREIGN") {
      const name = unquote(/^("(?:[^"]|"")*"|`[^`]*`|\[[^\]]*\]|\S+)/.exec(definition)![1]);
      const checks = checksIn(definition);
      if (checks.length) parsed.columnChecks.set(name, checks.join(" AND "));
      if (/\bUNIQUE\b/i.test(blankNested(definition))) parsed.columnUnique.add(name);
    }
  }
  return parsed;
}

function whereClause(indexSql: string | null): string | undefined {
  if (!indexSql) return undefined;
  const blanked = blankNested(indexSql);
  const at = blanked.search(/\bWHERE\b/i);
  return at === -1 ? undefined : indexSql.slice(at + 5).trim();
}

export interface IntrospectionResult {
  model: SchemaModel;
  /** Things in the database the model can't represent (views, expression indexes, ...). */
  skipped: string[];
}

function readColumns(db: DatabaseSync, table: string, skipped: string[]): ColumnRow[] {
  const info = rows<ColumnRow>(db, "SELECT * FROM pragma_table_xinfo(?)", table);
  for (const column of info.filter((c) => c.hidden !== 0)) skipped.push(`generated column ${table}.${column.name}`);
  return info.filter((c) => c.hidden === 0);
}

function toColumnModel(column: ColumnRow, parsed: ParsedCreate): ColumnModel {
  const model: ColumnModel = { name: column.name, type: sqlTypeFor(column.type ?? ""), notNull: column.notnull === 1 };
  if (column.dflt_value !== null) model.default = column.dflt_value;
  if (parsed.columnUnique.has(column.name)) model.unique = true;
  const check = parsed.columnChecks.get(column.name);
  if (check) model.check = check;
  return model;
}

function readForeignKeys(db: DatabaseSync, table: string): ForeignKeyModel[] {
  const fkRows = rows<ForeignKeyRow>(db, "SELECT * FROM pragma_foreign_key_list(?)", table);
  return [...new Set(fkRows.map((r) => r.id))].map((id) => {
    const group = fkRows.filter((r) => r.id === id).sort((a, b) => a.seq - b.seq);
    const parent = group[0].table;
    // A REFERENCES clause without columns points at the parent's primary key.
    const references = group.every((r) => r.to !== null)
      ? group.map((r) => r.to as string)
      : rows<{ name: string }>(db, "SELECT name FROM pragma_table_info(?) WHERE pk > 0 ORDER BY pk", parent).map((r) => r.name);
    return {
      columns: group.map((r) => r.from),
      table: parent,
      references,
      onDelete: group[0].on_delete,
      onUpdate: group[0].on_update
    };
  });
}

function readIndexes(db: DatabaseSync, table: string, master: MasterRow[], skipped: string[]): IndexModel[] {
  const indexes: IndexModel[] = [];
  for (const index of rows<IndexListRow>(db, "SELECT * FROM pragma_index_list(?)", table)) {
    if (index.origin !== "c") continue;
    const columns = rows<IndexInfoRow>(db, "SELECT * FROM pragma_index_info(?) ORDER BY seqno", index.name).map((r) => r.name);
    if (columns.includes(null)) {
      skipped.push(`expression index ${index.name}`);
      continue;
    }
    const where = whereClause(master.find((row) => row.type === "index" && row.name === index.name)?.sql ?? null);
    indexes.push({ name: index.name, columns: columns as string[], unique: index.unique === 1, ...(where ? { where } : {}) });
  }
  return indexes;
}

function readTable(db: DatabaseSync, entry: MasterRow, master: MasterRow[], skipped: string[]): TableModel {
  const parsed = parseCreateTable(entry.sql ?? "");
  const columns = readColumns(db, entry.name, skipped);
  const keyColumns = columns
    .filter((c) => c.pk > 0)
    .sort((a, b) => a.pk - b.pk)
    .map((c) => c.name);
  return {
    name: entry.name,
    columns: columns.map((c) => toColumnModel(c, parsed)),
    ...(keyColumns.length
      ? { primaryKey: { columns: keyColumns, autoIncrement: keyColumns.length === 1 && parsed.autoIncrement } }
      : {}),
    foreignKeys: readForeignKeys(db, entry.name),
    indexes: readIndexes(db, entry.name, master, skipped),
    uniques: parsed.uniques,
    checks: parsed.checks,
    triggers: master
      .filter((row) => row.type === "trigger" && row.tbl_name === entry.name)
      .map((row) => ({ name: row.name, sql: row.sql ?? "" }))
  };
}

export function introspect(db: DatabaseSync): IntrospectionResult {
  const skipped: string[] = [];
  const master = rows<MasterRow>(
    db,
    "SELECT type, name, tbl_name, sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name"
  );
  for (const view of master.filter((row) => row.type === "view")) skipped.push(`view ${view.name}`);
  const tables = master
    .filter((row) => row.type === "table" && row.name !== "_chain_migrations")
    .map((entry) => readTable(db, entry, master, skipped));
  return { model: { tables }, skipped };
}
