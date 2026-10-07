import fs from "node:fs";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";

import { currentModel, list } from "./migration.js";
import { checkChainApp, resolveDbPath } from "./nativeProject.js";
import { emitSchema } from "./schema/emit-classes.js";
import { applyDown, applyUp, ensureHistoryTable, readHistory, withMigrationSettings, type Migration } from "./schema/history.js";
import { introspect } from "./schema/introspect.js";
import { findDbDirs, loadMigrations, migrationFiles, slugify, writeModel, type DbDirs } from "./schema/migrations-dir.js";
import { canonicalize, comparable, modelsEqual } from "./schema/model.js";

// `chain database …` — EF Core's `dotnet ef database update` and
// `dbcontext scaffold`, operating on the app's real SQLite file (the one
// desktop.storage opens) without launching the app.

function fail(message: string): never {
  console.error(`Error: ${message}`);
  process.exit(1);
}

function requireDbDirs(cwd: string): DbDirs {
  checkChainApp(cwd);
  return findDbDirs(cwd) ?? fail("no db/migrations folder yet — run `chain migration add <name>` to create one.");
}

function targetVersion(value: string | undefined, migrations: Migration[]): number {
  if (value === undefined) return migrations.at(-1)?.version ?? 0;
  if (/^\d+$/.test(value)) return Number(value);
  return migrations.find((m) => m.name === slugify(value))?.version ?? fail(`no migration named \`${value}\`.`);
}

/** `chain database update [target]` — up to the latest (or target), or down to target, like EF. */
async function update(cwd: string, args: string[]): Promise<void> {
  const dirs = requireDbDirs(cwd);
  const migrations = await loadMigrations(dirs.migrationsDir);
  const target = targetVersion(args[0], migrations);
  const dbPath = resolveDbPath(cwd);
  fs.mkdirSync(path.dirname(dbPath), { recursive: true });
  const db = new DatabaseSync(dbPath);
  try {
    ensureHistoryTable(db);
    const applied = new Set(readHistory(db).map((row) => row.version));
    const up = migrations.filter((m) => m.version <= target && !applied.has(m.version));
    const down = migrations.filter((m) => m.version > target && applied.has(m.version)).reverse();
    const cantRevert = down.filter((m) => !m.down);
    if (cantRevert.length) {
      fail(
        `can't go back to ${target}: ${cantRevert.map((m) => m.version).join(", ")} ` +
          "were written without a down script. Only migrations with `down` can be reverted."
      );
    }
    if (!up.length && !down.length) {
      console.log("Already up to date — no pending migrations.");
      return;
    }
    withMigrationSettings(db, () => {
      for (const m of down) {
        applyDown(db, m);
        console.log(`Reverted ${String(m.version).padStart(4, "0")} ${m.name ?? ""}`);
      }
      for (const m of up) {
        applyUp(db, m);
        console.log(`Applied  ${String(m.version).padStart(4, "0")} ${m.name ?? ""}`);
      }
    });
    console.log(`\n${dbPath} is now at version ${target}.`);
  } catch (error) {
    fail(`migration failed — ${(error as Error).message}. That migration was rolled back; earlier ones stay applied.`);
  } finally {
    db.close();
  }
}

/**
 * `chain database scaffold` — adopts an app whose migrations were written by
 * hand, like EF's `dbcontext scaffold`. Replays every migration into an
 * in-memory database (never the real one), reads the resulting schema back,
 * writes it as @Table classes, and saves it as the latest migration's model,
 * so the next `chain migration add` diffs from exactly here.
 */
async function scaffold(cwd: string, args: string[]): Promise<void> {
  const dirs = requireDbDirs(cwd);
  const migrations = await loadMigrations(dirs.migrationsDir);
  const latest = migrationFiles(dirs.migrationsDir).at(-1) ?? fail("there are no migrations to scaffold from.");

  const db = new DatabaseSync(":memory:");
  ensureHistoryTable(db);
  withMigrationSettings(db, () => migrations.forEach((m) => applyUp(db, m)));
  const { model, skipped } = introspect(db);
  db.close();

  const files = emitSchema(model);
  fs.mkdirSync(dirs.schemaDir, { recursive: true });
  const existing = files.filter((f) => fs.existsSync(path.join(dirs.schemaDir, f.fileName)));
  if (existing.length && !args.includes("--force")) {
    fail(
      `these files in ${path.relative(cwd, dirs.schemaDir)}/ would be replaced: ${existing.map((f) => f.fileName).join(", ")}.\n` +
        "Commit or move them, then rerun with `--force`."
    );
  }
  for (const file of files) fs.writeFileSync(path.join(dirs.schemaDir, file.fileName), file.source);

  const fromClasses = currentModel(dirs);
  if (!modelsEqual(model, fromClasses)) {
    const expected = JSON.stringify(comparable(canonicalize(model)));
    const actual = JSON.stringify(comparable(canonicalize(fromClasses)));
    fail(`the generated classes don't read back as the same schema — please report this.\n  database: ${expected}\n  classes:  ${actual}`);
  }
  const modelFile = writeModel(dirs.migrationsDir, latest, model);

  console.log(`Scaffolded ${model.tables.length} table classes into ${path.relative(cwd, dirs.schemaDir)}/ from ${migrations.length} migrations.`);
  console.log(`Saved the schema as of ${latest.fileName} to ${modelFile}.`);
  for (const item of skipped) console.log(`  ! Not represented in the classes: ${item}. Keep managing it with --empty migrations.`);
  console.log("\nFrom now on, change the classes and run `chain migration add <name>`.");
}

export async function database(args: string[]): Promise<void> {
  const [subcommand, ...rest] = args;
  const cwd = process.cwd();
  switch (subcommand) {
    case "update":
      await update(cwd, rest);
      break;
    case "list":
      await list();
      break;
    case "scaffold":
      await scaffold(cwd, rest);
      break;
    default:
      console.error(
        "Usage:\n" +
          "  chain database update [target]    Apply pending migrations, or revert down to a version or name\n" +
          "  chain database list               Same as `chain migration list`\n" +
          "  chain database scaffold [--force] Generate schema classes from existing hand-written migrations"
      );
      process.exit(1);
  }
}
