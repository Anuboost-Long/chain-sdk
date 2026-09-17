import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { checkChainApp, resolveDbPath } from "./nativeProject.js";
import { findMigrationsDir } from "./migration.js";

interface Migration {
  version: number;
  sql: string;
}

function requireMigrationsDir(cwd: string): string {
  checkChainApp(cwd);
  const migrationsDir = findMigrationsDir(path.join(cwd, "src"));
  if (!migrationsDir) {
    console.error("Error: no db/migrations found — run `chain migration <name>` first.");
    process.exit(1);
  }
  return migrationsDir;
}

/** Loads `migrations/index.ts` the same way the app itself would, via
 * Node's native TypeScript support — these files are plain object
 * literals behind type-only imports, well within what type-stripping
 * handles, so no bundler/transpiler is needed. */
async function loadMigrations(migrationsDir: string): Promise<Migration[]> {
  const indexPath = path.join(migrationsDir, "index.ts");
  const mod = (await import(pathToFileURL(indexPath).href)) as { migrations: Migration[] };
  return [...mod.migrations].sort((a, b) => a.version - b.version);
}

function fileNameFor(migrationsDir: string, version: number): string {
  const match = fs
    .readdirSync(migrationsDir)
    .find((file) => file.startsWith(`${String(version).padStart(4, "0")}-`));
  return match ?? `${String(version).padStart(4, "0")}-*.ts`;
}

async function update(cwd: string): Promise<void> {
  const migrationsDir = requireMigrationsDir(cwd);
  const migrations = await loadMigrations(migrationsDir);
  const dbPath = resolveDbPath(cwd);

  const { DatabaseSync } = await import("node:sqlite");
  fs.mkdirSync(path.dirname(dbPath), { recursive: true });
  const db = new DatabaseSync(dbPath);
  try {
    // Mirrors crates/core/src/storage.rs's Database::open/migrate exactly
    // — chain database update and the app's own desktop.storage.migrate()
    // must be indistinguishable to whichever one runs second.
    db.exec(
      "CREATE TABLE IF NOT EXISTS _chain_migrations (" +
        "version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT (datetime('now')))"
    );
    const applied = new Set(
      db.prepare("SELECT version FROM _chain_migrations").all().map((row) => row.version as number)
    );

    const pending = migrations.filter((m) => !applied.has(m.version));
    if (!pending.length) {
      console.log("Already up to date — no pending migrations.");
      return;
    }

    for (const migration of pending) {
      db.exec(migration.sql);
      db.prepare("INSERT INTO _chain_migrations (version) VALUES (?)").run(migration.version);
      console.log(`Applied ${fileNameFor(migrationsDir, migration.version)}.`);
    }
    console.log(`\n${pending.length} migration(s) applied to ${dbPath}.`);
  } catch (error) {
    console.error(`Error: migration failed — ${(error as Error).message}`);
    process.exit(1);
  } finally {
    db.close();
  }
}

async function list(cwd: string): Promise<void> {
  const migrationsDir = requireMigrationsDir(cwd);
  const migrations = await loadMigrations(migrationsDir);
  const dbPath = resolveDbPath(cwd);

  let applied = new Map<number, string>();
  if (fs.existsSync(dbPath)) {
    const { DatabaseSync } = await import("node:sqlite");
    const db = new DatabaseSync(dbPath, { readOnly: true });
    try {
      const rows = db.prepare("SELECT version, applied_at FROM _chain_migrations").all();
      applied = new Map(rows.map((row) => [row.version as number, row.applied_at as string]));
    } catch {
      // _chain_migrations doesn't exist yet — app.db predates any migrate() call.
    } finally {
      db.close();
    }
  }

  if (!migrations.length) {
    console.log("No migrations found.");
    return;
  }
  for (const migration of migrations) {
    const fileName = fileNameFor(migrationsDir, migration.version);
    const appliedAt = applied.get(migration.version);
    console.log(appliedAt ? `  [applied ${appliedAt}] ${fileName}` : `  [pending]          ${fileName}`);
  }
}

export async function database(args: string[]): Promise<void> {
  const [subcommand] = args;
  const cwd = process.cwd();
  switch (subcommand) {
    case "update":
      await update(cwd);
      break;
    case "list":
      await list(cwd);
      break;
    default:
      console.error("Usage: chain database <update|list>");
      process.exit(1);
  }
}
