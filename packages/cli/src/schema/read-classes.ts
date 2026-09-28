// Reads the app's `@Table` classes into a schema model with the TypeScript
// compiler, the way EF reads a C# model: nothing in the schema files runs.
// A property's declared type decides its SQLite type (string → TEXT,
// number/boolean/bigint → INTEGER, Uint8Array → BLOB), `| null`/`?` makes it
// nullable, and decorator arguments are read as literals. The decorators in
// @chain/sdk/schema are markers with no runtime behavior.

import fs from "node:fs";
import path from "node:path";
import ts from "typescript";

import {
  snakeCase,
  type ColumnModel,
  type ForeignKeyModel,
  type IndexModel,
  type SchemaModel,
  type SqlType,
  type TableModel
} from "./model.js";

export class SchemaError extends Error {}

type Literal = string | number | boolean | null | Literal[] | { [key: string]: Literal } | ts.ArrowFunction;

interface ParsedClass {
  node: ts.ClassDeclaration;
  table: TableModel;
  /** Properties with an unresolved `@ForeignKey`, finished once every class is known. */
  pendingForeignKeys: { column: string; target: ts.Expression; options: Record<string, Literal> }[];
}

function where(node: ts.Node): string {
  const file = node.getSourceFile();
  const { line } = file.getLineAndCharacterOfPosition(node.getStart());
  return `${path.relative(process.cwd(), file.fileName)}:${line + 1}`;
}

function evaluateObject(expr: ts.ObjectLiteralExpression): Record<string, Literal> {
  const out: Record<string, Literal> = {};
  for (const property of expr.properties) {
    if (!ts.isPropertyAssignment(property) || !(ts.isIdentifier(property.name) || ts.isStringLiteral(property.name))) {
      throw new SchemaError(`${where(property)}: decorator options must be plain \`key: value\` pairs.`);
    }
    out[property.name.text] = evaluate(property.initializer);
  }
  return out;
}

function evaluateScalar(expr: ts.Expression): Literal | undefined {
  if (ts.isStringLiteral(expr) || ts.isNoSubstitutionTemplateLiteral(expr)) return expr.text;
  if (ts.isNumericLiteral(expr)) return Number(expr.text);
  if (ts.isPrefixUnaryExpression(expr) && expr.operator === ts.SyntaxKind.MinusToken && ts.isNumericLiteral(expr.operand)) {
    return -Number(expr.operand.text);
  }
  if (expr.kind === ts.SyntaxKind.TrueKeyword) return true;
  if (expr.kind === ts.SyntaxKind.FalseKeyword) return false;
  if (expr.kind === ts.SyntaxKind.NullKeyword) return null;
  return undefined;
}

function evaluate(expr: ts.Expression): Literal {
  const scalar = evaluateScalar(expr);
  if (scalar !== undefined) return scalar;
  if (ts.isArrayLiteralExpression(expr)) return expr.elements.map(evaluate);
  if (ts.isArrowFunction(expr)) return expr;
  if (ts.isAsExpression(expr) || ts.isParenthesizedExpression(expr)) return evaluate(expr.expression);
  if (ts.isObjectLiteralExpression(expr)) return evaluateObject(expr);
  throw new SchemaError(`${where(expr)}: decorator arguments must be literals, not \`${expr.getText()}\`.`);
}

interface Decorator {
  name: string;
  args: ts.NodeArray<ts.Expression>;
  node: ts.Decorator;
}

function decoratorsOf(node: ts.HasDecorators): Decorator[] {
  return (ts.getDecorators(node) ?? []).flatMap((decorator) => {
    const call = decorator.expression;
    if (!ts.isCallExpression(call) || !ts.isIdentifier(call.expression)) return [];
    return [{ name: call.expression.text, args: call.arguments, node: decorator }];
  });
}

function options(decorator: Decorator, at: number): Record<string, Literal> {
  const arg = decorator.args[at];
  if (!arg) return {};
  const value = evaluate(arg);
  if (value === null || typeof value !== "object" || Array.isArray(value) || ts.isArrowFunction(value as ts.Node)) {
    throw new SchemaError(`${where(arg)}: expected an options object.`);
  }
  return value as Record<string, Literal>;
}

function stringOption(opts: Record<string, Literal>, key: string, node: ts.Node): string | undefined {
  const value = opts[key];
  if (value === undefined) return undefined;
  if (typeof value !== "string") throw new SchemaError(`${where(node)}: \`${key}\` must be a string.`);
  return value;
}

function stringListOption(opts: Record<string, Literal>, key: string, node: ts.Node): string[] | undefined {
  const value = opts[key];
  if (value === undefined) return undefined;
  if (!Array.isArray(value) || !value.every((v) => typeof v === "string")) {
    throw new SchemaError(`${where(node)}: \`${key}\` must be an array of strings.`);
  }
  return value as string[];
}

const sqlTypes: Record<string, SqlType> = { integer: "INTEGER", text: "TEXT", real: "REAL", blob: "BLOB", numeric: "NUMERIC" };

/** The SQLite type and nullability a property's TypeScript type maps to. */
function mapType(checker: ts.TypeChecker, property: ts.PropertyDeclaration): { type?: SqlType; nullable: boolean } {
  const type = checker.getTypeAtLocation(property);
  const parts = type.isUnion() ? type.types : [type];
  const nullable =
    Boolean(property.questionToken) ||
    parts.some((t) => t.flags & (ts.TypeFlags.Null | ts.TypeFlags.Undefined));
  const kinds = new Set<SqlType>();
  for (const part of parts) {
    const flags = part.flags;
    if (flags & (ts.TypeFlags.Null | ts.TypeFlags.Undefined)) continue;
    if (flags & ts.TypeFlags.StringLike) kinds.add("TEXT");
    else if (flags & (ts.TypeFlags.NumberLike | ts.TypeFlags.BooleanLike | ts.TypeFlags.BigIntLike)) kinds.add("INTEGER");
    else if (["Uint8Array", "ArrayBuffer"].includes(part.getSymbol()?.getName() ?? "")) kinds.add("BLOB");
    else return { nullable };
  }
  return { type: kinds.size === 1 ? [...kinds][0] : undefined, nullable };
}

function defaultFrom(opts: Record<string, Literal>, node: ts.Node): string | undefined {
  const sql = stringOption(opts, "defaultSql", node);
  if (sql !== undefined) return sql;
  const value = opts.default;
  if (value === undefined) return undefined;
  if (value === null) return "NULL";
  if (typeof value === "boolean") return value ? "1" : "0";
  if (typeof value === "number") return String(value);
  if (typeof value === "string") return `'${value.replaceAll("'", "''")}'`;
  throw new SchemaError(`${where(node)}: \`default\` must be a string, number, boolean, or null.`);
}

function columnFrom(checker: ts.TypeChecker, property: ts.PropertyDeclaration, table: string, decorators: Decorator[]): ColumnModel {
  const columnDecorator = decorators.find((d) => d.name === "Column");
  const columnOptions = columnDecorator ? options(columnDecorator, 0) : {};
  const name = stringOption(columnOptions, "name", property) ?? (property.name as ts.Identifier | ts.StringLiteral).text;
  const mapped = mapType(checker, property);
  const explicitType = stringOption(columnOptions, "type", property);
  const type = explicitType ? sqlTypes[explicitType] : mapped.type;
  if (!type) {
    throw new SchemaError(
      `${where(property)}: can't map the type of \`${table}.${name}\` to SQLite. ` +
        'Use string, number, boolean, bigint, or Uint8Array, or set `@Column({ type: "..." })`.'
    );
  }
  const column: ColumnModel = { name, type, notNull: !mapped.nullable };
  const defaultValue = defaultFrom(columnOptions, property);
  if (defaultValue !== undefined) column.default = defaultValue;
  if (columnOptions.unique === true || decorators.some((d) => d.name === "Unique")) column.unique = true;
  const check = stringOption(columnOptions, "check", property);
  if (check) column.check = check;
  return column;
}

function readProperty(
  checker: ts.TypeChecker,
  property: ts.PropertyDeclaration,
  parsed: ParsedClass,
  keyColumns: { column: string; autoIncrement: boolean }[]
): void {
  const decorators = decoratorsOf(property);
  if (decorators.some((d) => d.name === "NotMapped")) return;
  if (property.modifiers?.some((m) => m.kind === ts.SyntaxKind.StaticKeyword)) return;
  if (!ts.isIdentifier(property.name) && !ts.isStringLiteral(property.name)) {
    throw new SchemaError(`${where(property)}: column properties need a plain name.`);
  }
  const column = columnFrom(checker, property, parsed.table.name, decorators);
  parsed.table.columns.push(column);

  for (const decorator of decorators) {
    if (decorator.name === "PrimaryKey") {
      keyColumns.push({ column: column.name, autoIncrement: options(decorator, 0).autoIncrement === true });
    } else if (decorator.name === "ForeignKey") {
      if (!decorator.args[0]) throw new SchemaError(`${where(decorator.node)}: @ForeignKey needs a target, like \`() => Course\`.`);
      parsed.pendingForeignKeys.push({ column: column.name, target: decorator.args[0], options: options(decorator, 1) });
    } else if (decorator.name === "Index") {
      parsed.table.indexes.push(indexFrom(options(decorator, 0), [column.name], parsed.table.name, decorator.node));
    }
  }
}

function indexFrom(opts: Record<string, Literal>, columns: string[], table: string, node: ts.Node): IndexModel {
  const unique = opts.unique === true;
  const indexWhere = stringOption(opts, "where", node);
  return {
    name: stringOption(opts, "name", node) ?? `${unique ? "uq" : "idx"}_${table}_${columns.join("_")}`,
    columns,
    unique,
    ...(indexWhere ? { where: indexWhere } : {})
  };
}

function tableFrom(node: ts.ClassDeclaration, tableDecorator: Decorator): TableModel {
  const nameArg = tableDecorator.args[0] ? evaluate(tableDecorator.args[0]) : undefined;
  if (nameArg !== undefined && typeof nameArg !== "string") {
    throw new SchemaError(`${where(tableDecorator.node)}: the table name must be a string.`);
  }
  const tableOptions = options(tableDecorator, 1);
  const uniques = tableOptions.unique ?? [];
  if (!Array.isArray(uniques) || !uniques.every((u) => Array.isArray(u) && u.every((c) => typeof c === "string"))) {
    throw new SchemaError(`${where(tableDecorator.node)}: \`unique\` must be a list of column lists.`);
  }
  return {
    name: nameArg ?? snakeCase(node.name!.text),
    columns: [],
    foreignKeys: [],
    indexes: [],
    uniques: uniques as string[][],
    checks: stringListOption(tableOptions, "checks", tableDecorator.node) ?? [],
    triggers: []
  };
}

function applyClassDecorators(table: TableModel, decorators: Decorator[]): void {
  for (const decorator of decorators) {
    if (decorator.name === "Index") {
      const opts = options(decorator, 0);
      const columns = stringListOption(opts, "columns", decorator.node);
      if (!columns?.length) throw new SchemaError(`${where(decorator.node)}: a class-level @Index needs \`columns\`.`);
      table.indexes.push(indexFrom(opts, columns, table.name, decorator.node));
    } else if (decorator.name === "Trigger") {
      const [name, sql] = decorator.args.map(evaluate);
      if (typeof name !== "string" || typeof sql !== "string") {
        throw new SchemaError(`${where(decorator.node)}: @Trigger takes a name and the full CREATE TRIGGER statement, as strings.`);
      }
      table.triggers.push({ name, sql });
    }
  }
}

/** Marked @PrimaryKey columns, or by convention a column named `id`. */
function primaryKeyOf(table: TableModel, keyColumns: { column: string; autoIncrement: boolean }[]): TableModel["primaryKey"] {
  if (keyColumns.length) {
    return { columns: keyColumns.map((k) => k.column), autoIncrement: keyColumns.length === 1 && keyColumns[0].autoIncrement };
  }
  return table.columns.some((c) => c.name === "id") ? { columns: ["id"], autoIncrement: false } : undefined;
}

function readClass(checker: ts.TypeChecker, node: ts.ClassDeclaration): ParsedClass | undefined {
  const decorators = decoratorsOf(node);
  const tableDecorator = decorators.find((d) => d.name === "Table");
  if (!tableDecorator) return undefined;
  if (!node.name) throw new SchemaError(`${where(node)}: @Table classes need a name.`);

  const parsed: ParsedClass = { node, table: tableFrom(node, tableDecorator), pendingForeignKeys: [] };
  const keyColumns: { column: string; autoIncrement: boolean }[] = [];
  for (const member of node.members) {
    if (ts.isPropertyDeclaration(member)) readProperty(checker, member, parsed, keyColumns);
  }
  applyClassDecorators(parsed.table, decorators);
  const primaryKey = primaryKeyOf(parsed.table, keyColumns);
  if (primaryKey) parsed.table.primaryKey = primaryKey;
  return parsed;
}

function resolveForeignKey(checker: ts.TypeChecker, classes: ParsedClass[], pending: ParsedClass["pendingForeignKeys"][number]): ForeignKeyModel {
  const arrow = pending.target;
  if (!ts.isArrowFunction(arrow) || !ts.isIdentifier(arrow.body)) {
    throw new SchemaError(`${where(arrow)}: @ForeignKey's target must look like \`() => Course\`.`);
  }
  let symbol = checker.getSymbolAtLocation(arrow.body);
  if (symbol && symbol.flags & ts.SymbolFlags.Alias) symbol = checker.getAliasedSymbol(symbol);
  const target = classes.find((c) => symbol?.declarations?.includes(c.node));
  if (!target) throw new SchemaError(`${where(arrow)}: \`${arrow.body.text}\` isn't a @Table class in the schema folder.`);
  const referenced = stringOption(pending.options, "column", arrow);
  const key = target.table.primaryKey?.columns;
  if (!referenced && key?.length !== 1) {
    throw new SchemaError(`${where(arrow)}: ${target.table.name} has no single-column primary key; set \`column\`.`);
  }
  const fk: ForeignKeyModel = { columns: [pending.column], table: target.table.name, references: [referenced ?? key![0]] };
  const onDelete = stringOption(pending.options, "onDelete", arrow);
  const onUpdate = stringOption(pending.options, "onUpdate", arrow);
  if (onDelete) fk.onDelete = onDelete.toUpperCase();
  if (onUpdate) fk.onUpdate = onUpdate.toUpperCase();
  return fk;
}

function resolveForeignKeys(checker: ts.TypeChecker, classes: ParsedClass[]): void {
  for (const parsed of classes) {
    parsed.table.foreignKeys.push(...parsed.pendingForeignKeys.map((pending) => resolveForeignKey(checker, classes, pending)));
  }
}

function schemaFiles(dir: string): string[] {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return schemaFiles(full);
    return /\.tsx?$/.test(entry.name) && !entry.name.endsWith(".d.ts") ? [full] : [];
  });
}

/** Every `@Table` class under `schemaDir`, as a model. Throws `SchemaError` with file:line on anything it can't read. */
export function readSchemaClasses(schemaDir: string): SchemaModel {
  const program = ts.createProgram(schemaFiles(schemaDir), {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    allowImportingTsExtensions: true,
    strict: true,
    noEmit: true,
    skipLibCheck: true
  });
  const checker = program.getTypeChecker();
  const classes: ParsedClass[] = [];
  for (const file of program.getSourceFiles()) {
    if (!path.resolve(file.fileName).startsWith(path.resolve(schemaDir))) continue;
    ts.forEachChild(file, (node) => {
      if (ts.isClassDeclaration(node)) {
        const parsed = readClass(checker, node);
        if (parsed) classes.push(parsed);
      }
    });
  }
  resolveForeignKeys(checker, classes);
  const seen = new Map<string, ParsedClass>();
  for (const parsed of classes) {
    const duplicate = seen.get(parsed.table.name);
    if (duplicate) throw new SchemaError(`${where(parsed.node)}: table ${parsed.table.name} is also declared at ${where(duplicate.node)}.`);
    seen.set(parsed.table.name, parsed);
  }
  return { tables: classes.map((c) => c.table) };
}
