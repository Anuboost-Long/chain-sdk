// Writes `@Table` class files from a schema model — the output of
// `chain database scaffold`, like EF's `dbcontext scaffold`. read-classes.ts
// must read these files back into the exact same model.

import { canonicalize, pascalCase, snakeCase, type ColumnModel, type SchemaModel, type TableModel } from "./model.js";

export interface EmittedFile {
  fileName: string;
  source: string;
}

const inferredType: Record<ColumnModel["type"], string> = {
  INTEGER: "number",
  TEXT: "string",
  REAL: "number",
  NUMERIC: "number",
  BLOB: "Uint8Array"
};

function literal(value: unknown): string {
  return Array.isArray(value) ? `[${value.map(literal).join(", ")}]` : JSON.stringify(value);
}

function template(sql: string): string {
  return `\`${sql.replaceAll("\\", "\\\\").replaceAll("`", "\\`").replaceAll("${", "\\${")}\``;
}

function objectLiteral(entries: [string, string | undefined][]): string {
  const present = entries.filter(([, value]) => value !== undefined).map(([key, value]) => key + ": " + value);
  return present.length ? `{ ${present.join(", ")} }` : "";
}

/** A SQL default as a `@Column` option: a JS literal where one round-trips exactly, `defaultSql` otherwise. */
function defaultOption(value: string): [string, string] {
  if (/^-?\d+(?:\.\d+)?$/.test(value) && String(Number(value)) === value) return ["default", value];
  if (/^'(?:[^']|'')*'$/.test(value)) return ["default", literal(value.slice(1, -1).replaceAll("''", "'"))];
  if (/^NULL$/i.test(value)) return ["default", "null"];
  return ["defaultSql", literal(value)];
}

export function className(table: string): string {
  return pascalCase(table) || "Table";
}

export function fileNameFor(table: string): string {
  return `${snakeCase(table).replaceAll("_", "-")}.ts`;
}

function propertyName(name: string): string {
  return /^[A-Za-z_$][\w$]*$/.test(name) ? name : literal(name);
}

/** Decorator names and sibling-table imports one file needs. */
interface Needs {
  decorators: Set<string>;
  tables: Set<string>;
}

function classDecorators(table: TableModel, needs: Needs): string[] {
  const tableOptions = objectLiteral([
    ["checks", table.checks.length ? literal(table.checks) : undefined],
    ["unique", table.uniques.length ? literal(table.uniques) : undefined]
  ]);
  const conventional = snakeCase(className(table.name)) === table.name && !tableOptions;
  const decorators = [`@Table(${conventional ? "" : [literal(table.name), tableOptions].filter(Boolean).join(", ")})`];
  for (const index of table.indexes.filter((i) => i.columns.length > 1)) {
    needs.decorators.add("Index");
    const options = objectLiteral([
      ["name", literal(index.name)],
      ["columns", literal(index.columns)],
      ["unique", index.unique ? "true" : undefined],
      ["where", index.where ? literal(index.where) : undefined]
    ]);
    decorators.push(`@Index(${options})`);
  }
  for (const trigger of table.triggers) {
    needs.decorators.add("Trigger");
    decorators.push(`@Trigger(${literal(trigger.name)}, ${template(trigger.sql)})`);
  }
  return decorators;
}

function foreignKeyDecorator(table: TableModel, column: ColumnModel, model: SchemaModel, needs: Needs): string | undefined {
  const fk = table.foreignKeys.find((f) => f.columns.length === 1 && f.columns[0] === column.name);
  if (!fk) return undefined;
  const target = model.tables.find((t) => t.name === fk.table);
  if (!target) throw new Error(`${table.name}.${column.name} references ${fk.table}, which isn't a table in this database.`);
  const targetKey = target.primaryKey?.columns;
  const referencesKey = targetKey?.length === 1 && targetKey[0] === fk.references[0];
  needs.decorators.add("ForeignKey");
  if (target.name !== table.name) needs.tables.add(target.name);
  const options = objectLiteral([
    ["column", referencesKey ? undefined : literal(fk.references[0])],
    ["onDelete", fk.onDelete ? literal(fk.onDelete.toLowerCase()) : undefined],
    ["onUpdate", fk.onUpdate ? literal(fk.onUpdate.toLowerCase()) : undefined]
  ]);
  return `@ForeignKey(() => ${className(target.name)}${options ? ", " + options : ""})`;
}

function columnDecorators(table: TableModel, column: ColumnModel, model: SchemaModel, needs: Needs): string[] {
  const decorators: string[] = [];
  const pk = table.primaryKey;
  if (pk?.columns.includes(column.name)) {
    needs.decorators.add("PrimaryKey");
    decorators.push(`@PrimaryKey(${pk.autoIncrement ? "{ autoIncrement: true }" : ""})`);
  }
  const fk = foreignKeyDecorator(table, column, model, needs);
  if (fk) decorators.push(fk);
  for (const index of table.indexes.filter((i) => i.columns.length === 1 && i.columns[0] === column.name)) {
    needs.decorators.add("Index");
    const options = objectLiteral([
      ["name", literal(index.name)],
      ["unique", index.unique ? "true" : undefined],
      ["where", index.where ? literal(index.where) : undefined]
    ]);
    decorators.push(`@Index(${options})`);
  }
  const explicitType = inferredType[column.type] === "number" && column.type !== "INTEGER";
  const options = objectLiteral([
    ["type", explicitType ? literal(column.type.toLowerCase()) : undefined],
    ...(column.default === undefined ? [] : [defaultOption(column.default)]),
    ["unique", column.unique ? "true" : undefined],
    ["check", column.check ? literal(column.check) : undefined]
  ]);
  if (options) {
    needs.decorators.add("Column");
    decorators.push(`@Column(${options})`);
  }
  return decorators;
}

function propertyLine(table: TableModel, column: ColumnModel): string {
  const key = table.primaryKey?.columns;
  const rowidKey = key?.length === 1 && key[0] === column.name && column.type === "INTEGER";
  const nullable = !column.notNull && !rowidKey;
  return `  ${propertyName(column.name)}!: ${inferredType[column.type]}${nullable ? " | null" : ""};`;
}

function moduleOf(table: string): string {
  return "./" + fileNameFor(table).replace(/\.ts$/, "");
}

function emitTable(table: TableModel, model: SchemaModel): EmittedFile {
  const needs: Needs = { decorators: new Set(["Table"]), tables: new Set() };
  const body = table.columns.map((column) =>
    [...columnDecorators(table, column, model, needs).map((d) => `  ${d}`), propertyLine(table, column)].join("\n")
  );
  const lines = [...classDecorators(table, needs), `export class ${className(table.name)} {`, body.join("\n\n"), "}"];
  const header = [
    `import { ${[...needs.decorators].sort((a, b) => a.localeCompare(b)).join(", ")} } from "@chain/sdk/schema";`,
    ...[...needs.tables]
      .sort((a, b) => a.localeCompare(b))
      .map((t) => `import { ${className(t)} } from "${moduleOf(t)}";`)
  ];
  return { fileName: fileNameFor(table.name), source: `${header.join("\n")}\n\n${lines.join("\n")}\n` };
}

export function emitSchema(raw: SchemaModel): EmittedFile[] {
  const model = canonicalize(raw);
  const files = model.tables.map((table) => emitTable(table, model));
  const index = model.tables
    .map((t) => `export { ${className(t.name)} } from "${moduleOf(t.name)}";`)
    .join("\n");
  files.push({
    fileName: "index.ts",
    source:
      "// The database schema: one @Table class per table. Edit a class, then run\n" +
      "// `chain migration add <name>` to generate the migration for the change.\n" +
      `${index}\n`
  });
  return files;
}
