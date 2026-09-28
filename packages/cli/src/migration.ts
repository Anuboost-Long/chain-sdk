import fs from "node:fs";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";

import { confirm } from "./doctor.js";
import { checkChainApp, resolveDbPath } from "./nativeProject.js";
import { planMigration, noRenames, type Renames } from "./schema/diff.js";
import { checksum, readHistory, type HistoryRow, type Migration } from "./schema/history.js";
import {
  emptyMigrationSource,
  findDbDirs,
  generatedMigrationSource,
  loadMigrations,
  migrationFiles,
  readModel,
  regenerateIndex,
  scaffoldDbDirs,
  slugify,
  writeModel,
  type DbDirs
} from "./schema/migrations-dir.js";
import type { SchemaModel } from "./schema/model.js";
import { readSchemaClasses, SchemaError } from "./schema/read-classes.js";

// `chain migration …` — EF Core's `dotnet ef migrations …` for Chain apps:
// the @Table classes in db/schema are the model, each migration stores the
// model it produced, and `add` diffs the classes against the latest one.
// See agent-docs/framework/command/README.md's migrations section.

function fail(message: string): never {
  console.error(`Error: ${message}`);
  process.exit(1);
}

function rel(file: string): string {
  return path.relative(process.cwd(), file);
}

function requireDbDirs(cwd: string): DbDirs {
  checkChainApp(cwd);
  return findDbDirs(cwd) ?? fail("no db/migrations folder yet — run `chain migration add <name>` to create one.");
}

/** The schema as of the latest migration, from its saved model. */
export function latestModel(dirs: DbDirs): SchemaModel {
  const files = migrationFiles(dirs.migrationsDir);
  const latest = files.at(-1);
  if (!latest) return { tables: [] };
  if (!latest.modelFile) {
    fail(
      `${latest.fileName} has no saved model (it predates \`chain migration add\`), so there's nothing to diff against.\n` +
        "Run `chain database scaffold` once to generate the schema classes and record the current schema."
    );
  }
  return readModel(dirs.migrationsDir, latest);
}

export function currentModel(dirs: DbDirs): SchemaModel {
  try {
    return readSchemaClasses(dirs.schemaDir);
  } catch (error) {
    if (error instanceof SchemaError) fail(error.message);
    throw error;
  }
}

/** The first line of each statement, e.g. `ALTER TABLE "note" ADD COLUMN "pinned" INTEGER;`. */
function summarize(sql: string): string[] {
  return sql
    .split("\n")
    .filter((line) => /^(?:CREATE|ALTER|DROP|-- Rebuild)/.test(line))
    .map((line) => line.replace(/ \($/, ""));
}

function flagValues(args: string[], flag: string): string[] {
  return args.flatMap((arg, i) => (arg === flag && args[i + 1] ? [args[i + 1]] : []));
}

/** Renames given as `--rename-table old=new` / `--rename-column table.old=new`. */
function flagRenames(args: string[]): Renames {
  const renames = noRenames();
  for (const pair of flagValues(args, "--rename-table")) {
    const [old, renamed] = pair.split("=");
    renames.tables.set(old, renamed);
  }
  for (const pair of flagValues(args, "--rename-column")) {
    const [target, renamed] = pair.split("=");
    const [table, old] = target.split(".");
    const map = renames.columns.get(table) ?? new Map<string, string>();
    map.set(renamed, old);
    renames.columns.set(table, map);
  }
  return renames;
}

/** A dropped table plus a new one might be a rename — ask, like drizzle-kit does. */
async function askTableRenames(from: SchemaModel, to: SchemaModel, renames: Renames): Promise<void> {
  const fromNames = new Set(from.tables.map((t) => t.name));
  const toNames = new Set(to.tables.map((t) => t.name));
  for (const dropped of from.tables.filter((t) => !toNames.has(t.name) && !renames.tables.has(t.name))) {
    const taken = new Set(renames.tables.values());
    const created = to.tables.filter((t) => !fromNames.has(t.name) && !taken.has(t.name));
    for (const candidate of created) {
      if (await confirm(`Table \`${dropped.name}\` is gone and \`${candidate.name}\` is new. Did you rename \`${dropped.name}\` to \`${candidate.name}\`?`)) {
        renames.tables.set(dropped.name, candidate.name);
        break;
      }
    }
  }
}

/** A dropped column plus a new one of the same type in the same table might be a rename. */
async function askColumnRenames(from: SchemaModel, to: SchemaModel, renames: Renames): Promise<void> {
  for (const table of to.tables) {
    const oldName = [...renames.tables].find(([, n]) => n === table.name)?.[0] ?? table.name;
    const before = from.tables.find((t) => t.name === oldName);
    if (!before) continue;
    const map = renames.columns.get(table.name) ?? new Map<string, string>();
    const claimed = () => new Set(map.values());
    const removed = before.columns.filter((c) => !table.columns.some((n) => n.name === c.name));
    const added = table.columns.filter((c) => !before.columns.some((o) => o.name === c.name) && !map.has(c.name));
    for (const column of added) {
      const candidate = removed.find((r) => r.type === column.type && !claimed().has(r.name));
      if (candidate && (await confirm(`In \`${table.name}\`, did you rename column \`${candidate.name}\` to \`${column.name}\`?`))) {
        map.set(column.name, candidate.name);
      }
    }
    if (map.size) renames.columns.set(table.name, map);
  }
}

async function resolveRenames(from: SchemaModel, to: SchemaModel, args: string[]): Promise<Renames> {
  const renames = flagRenames(args);
  await askTableRenames(from, to, renames);
  await askColumnRenames(from, to, renames);
  return renames;
}

async function add(args: string[]): Promise<void> {
  const name = args.find((arg, i) => !arg.startsWith("--") && !args[i - 1]?.startsWith("--rename"));
  const slug = name ? slugify(name) : "";
  if (!slug) fail("give the migration a name, e.g. `chain migration add add-tags`.");

  const cwd = process.cwd();
  checkChainApp(cwd);
  let dirs = findDbDirs(cwd);
  if (!dirs) {
    dirs = scaffoldDbDirs(cwd);
    console.log(`Created ${rel(dirs.dbDir)}/ — add @Table classes to ${rel(dirs.schemaDir)}/ (see its index.ts).\n`);
  }

  const from = latestModel(dirs);
  const version = (migrationFiles(dirs.migrationsDir).at(-1)?.version ?? 0) + 1;
  const fileBase = `${String(version).padStart(4, "0")}-${slug}`;
  const file = { version, slug, fileName: `${fileBase}.ts` };

  if (args.includes("--empty")) {
    fs.writeFileSync(path.join(dirs.migrationsDir, file.fileName), emptyMigrationSource(version, slug));
    writeModel(dirs.migrationsDir, file, from);
    regenerateIndex(dirs.migrationsDir);
    console.log(`Created ${rel(path.join(dirs.migrationsDir, file.fileName))} to fill in by hand.`);
    return;
  }

  const to = currentModel(dirs);
  if (!to.tables.length && !from.tables.length) {
    fail(`no @Table classes in ${rel(dirs.schemaDir)}/ yet. Add one, then run this again.`);
  }
  const plan = planMigration(from, to, await resolveRenames(from, to, args));
  if (plan.empty) {
    console.log("No schema changes since the last migration — nothing to add.");
    console.log("For a change the classes can't express, like moving data, use `--empty` and write the SQL yourself.");
    return;
  }

  fs.writeFileSync(path.join(dirs.migrationsDir, file.fileName), generatedMigrationSource(version, slug, plan.up, plan.down, plan.warnings));
  const modelFile = writeModel(dirs.migrationsDir, file, to);
  regenerateIndex(dirs.migrationsDir);

  console.log(`Created ${rel(path.join(dirs.migrationsDir, file.fileName))} (version ${version}):`);
  for (const line of summarize(plan.up)) console.log(`  ${line}`);
  if (plan.warnings.length) {
    console.log("\nReview before shipping — these can lose or change data:");
    for (const warning of plan.warnings) console.log(`  ! ${warning}`);
  }
  console.log(`\nSaved the resulting schema to ${modelFile}.`);
  console.log("Apply it with `chain database update`, or just start the app — `initDb()` runs pending migrations.");
}

function openHistory(cwd: string): HistoryRow[] {
  const dbPath = resolveDbPath(cwd);
  if (!fs.existsSync(dbPath)) return [];
  const db = new DatabaseSync(dbPath, { readOnly: true });
  try {
    return readHistory(db);
  } finally {
    db.close();
  }
}

async function remove(): Promise<void> {
  const cwd = process.cwd();
  const dirs = requireDbDirs(cwd);
  const latest = migrationFiles(dirs.migrationsDir).at(-1) ?? fail("there are no migrations to remove.");
  if (openHistory(cwd).some((row) => row.version === latest.version)) {
    const previous = migrationFiles(dirs.migrationsDir).at(-2)?.version ?? 0;
    fail(`${latest.fileName} is already applied to your local database. Revert it first with \`chain database update ${previous}\`.`);
  }
  fs.rmSync(path.join(dirs.migrationsDir, latest.fileName));
  if (latest.modelFile) fs.rmSync(path.join(dirs.migrationsDir, latest.modelFile));
  regenerateIndex(dirs.migrationsDir);
  const removed = latest.modelFile ? `${latest.fileName} and ${latest.modelFile}` : latest.fileName;
  console.log(`Removed ${removed}. The schema classes are unchanged.`);
}

export type MigrationStatus = "applied" | "pending" | "changed" | "missing";

/** Each migration with its state in the local database, plus history rows whose file is gone. */
export async function migrationStatus(cwd: string, dirs: DbDirs) {
  const migrations = await loadMigrations(dirs.migrationsDir);
  const history = new Map(openHistory(cwd).map((row) => [row.version, row]));
  const rows: { version: number; name: string; status: MigrationStatus; appliedAt?: string }[] = migrations.map((migration) => {
    const applied = history.get(migration.version);
    let status: MigrationStatus = "pending";
    if (applied) status = applied.checksum && applied.checksum !== checksum(migration.sql) ? "changed" : "applied";
    return { version: migration.version, name: migration.name ?? "", status, appliedAt: applied?.appliedAt };
  });
  for (const row of history.values()) {
    if (!migrations.some((m) => m.version === row.version)) {
      rows.push({ version: row.version, name: row.name ?? "", status: "missing", appliedAt: row.appliedAt });
    }
  }
  return rows.sort((a, b) => a.version - b.version);
}

export async function list(): Promise<void> {
  const cwd = process.cwd();
  const dirs = requireDbDirs(cwd);
  const rows = await migrationStatus(cwd, dirs);
  if (!rows.length) {
    console.log("No migrations yet. Add @Table classes, then run `chain migration add <name>`.");
    return;
  }
  const label: Record<MigrationStatus, string> = {
    applied: "applied",
    pending: "pending",
    changed: "changed",
    missing: "missing"
  };
  for (const row of rows) {
    const when = row.appliedAt ? `  ${row.appliedAt}` : "";
    console.log(`  ${label[row.status].padEnd(8)} ${String(row.version).padStart(4, "0")}  ${row.name}${when}`);
  }
  if (rows.some((r) => r.status === "changed")) {
    console.log("\n`changed`: the migration's SQL was edited after it ran here. Databases that already ran it won't pick up the edit.");
  }
  if (rows.some((r) => r.status === "missing")) {
    console.log("\n`missing`: applied to this database, but the migration file is gone.");
  }
}

function parseVersion(value: string | undefined, migrations: Migration[]): number | undefined {
  if (value === undefined) return undefined;
  if (/^\d+$/.test(value)) return Number(value);
  return migrations.find((m) => m.name === slugify(value))?.version ?? fail(`no migration named \`${value}\`.`);
}

async function script(args: string[]): Promise<void> {
  const dirs = requireDbDirs(process.cwd());
  const migrations = await loadMigrations(dirs.migrationsDir);
  const [fromArg, toArg] = args.filter((a) => !a.startsWith("--"));
  const from = parseVersion(fromArg, migrations) ?? 0;
  const to = parseVersion(toArg, migrations) ?? migrations.at(-1)?.version ?? 0;
  const out: string[] = [];
  if (to >= from) {
    for (const m of migrations.filter((m) => m.version > from && m.version <= to)) {
      const nameSql = m.name ? "'" + m.name + "'" : "NULL";
      out.push(
        `-- ${m.version} ${m.name ?? ""}`.trim(),
        m.sql.trim(),
        `INSERT INTO _chain_migrations (version, name, checksum) VALUES (${m.version}, ${nameSql}, '${checksum(m.sql)}');`
      );
    }
  } else {
    for (const m of migrations.filter((m) => m.version > to && m.version <= from).reverse()) {
      if (!m.down) fail(`migration ${m.version} has no down script, so it can't be reverted.`);
      out.push(`-- revert ${m.version} ${m.name ?? ""}`.trim(), m.down.trim(), `DELETE FROM _chain_migrations WHERE version = ${m.version};`);
    }
  }
  console.log(out.join("\n\n"));
}

function check(): void {
  const dirs = requireDbDirs(process.cwd());
  const plan = planMigration(latestModel(dirs), currentModel(dirs));
  if (plan.empty) {
    console.log("No pending schema changes — the classes match the latest migration.");
    return;
  }
  console.log("The schema classes have changes no migration covers yet:");
  for (const line of summarize(plan.up)) console.log(`  ${line}`);
  console.log("\nRun `chain migration add <name>` to create one.");
  process.exit(1);
}

export async function migration(args: string[]): Promise<void> {
  const [subcommand, ...rest] = args;
  switch (subcommand) {
    case "add":
      await add(rest);
      break;
    case "remove":
      await remove();
      break;
    case "list":
      await list();
      break;
    case "script":
      await script(rest);
      break;
    case "check":
      check();
      break;
    default:
      console.error(
        "Usage:\n" +
          "  chain migration add <name>        Generate a migration from the schema classes' changes\n" +
          "  chain migration add <name> --empty  An empty migration to write by hand\n" +
          "  chain migration remove            Delete the latest migration (if not applied locally)\n" +
          "  chain migration list              Show every migration and whether it's applied\n" +
          "  chain migration script [from] [to]  Print the SQL between two versions (down if to < from)\n" +
          "  chain migration check             Exit 1 if the classes have unmigrated changes"
      );
      process.exit(1);
  }
}
