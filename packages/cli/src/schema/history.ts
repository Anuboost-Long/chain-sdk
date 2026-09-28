// The `_chain_migrations` history table and the migration runner, CLI side.
// Mirrors crates/core/src/storage.rs exactly (table shape, checksum,
// connection settings, rollback on failure), so the app applying a
// migration and `chain database update` applying it are indistinguishable.

import type { DatabaseSync } from "node:sqlite";

export interface Migration {
  version: number;
  sql: string;
  name?: string;
  down?: string;
}

export interface HistoryRow {
  version: number;
  appliedAt: string;
  name: string | null;
  checksum: string | null;
}

/** FNV-1a 64-bit of the SQL's UTF-8 bytes, hex — same as storage.rs's migration_checksum. */
export function checksum(sql: string): string {
  let hash = 0xcbf29ce484222325n;
  for (const byte of new TextEncoder().encode(sql)) {
    hash ^= BigInt(byte);
    hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return hash.toString(16).padStart(16, "0");
}

export function ensureHistoryTable(db: DatabaseSync): void {
  db.exec(
    "CREATE TABLE IF NOT EXISTS _chain_migrations (" +
      "version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT (datetime('now')), name TEXT, checksum TEXT)"
  );
  const columns = new Set(
    (db.prepare("SELECT name FROM pragma_table_info('_chain_migrations')").all() as { name: string }[]).map((c) => c.name)
  );
  for (const column of ["name", "checksum"]) {
    if (!columns.has(column)) db.exec(`ALTER TABLE _chain_migrations ADD COLUMN ${column} TEXT`);
  }
}

/** Applied migrations, oldest first. Empty if the table doesn't exist yet. */
export function readHistory(db: DatabaseSync): HistoryRow[] {
  const exists = db.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_chain_migrations'").get();
  if (!exists) return [];
  const columns = new Set(
    (db.prepare("SELECT name FROM pragma_table_info('_chain_migrations')").all() as { name: string }[]).map((c) => c.name)
  );
  const extra = columns.has("checksum") ? ", name, checksum" : ", NULL AS name, NULL AS checksum";
  return (
    db.prepare(`SELECT version, applied_at${extra} FROM _chain_migrations ORDER BY version`).all() as {
      version: number;
      applied_at: string;
      name: string | null;
      checksum: string | null;
    }[]
  ).map((row) => ({ version: row.version, appliedAt: row.applied_at, name: row.name, checksum: row.checksum }));
}

function flag(db: DatabaseSync, pragma: string): boolean {
  return Object.values(db.prepare(`PRAGMA ${pragma}`).get() as Record<string, number>)[0] === 1;
}

function runStep(db: DatabaseSync, label: string, sql: string, record: () => void): void {
  db.exec("PRAGMA legacy_alter_table = ON");
  try {
    db.exec(sql);
  } catch (error) {
    if (db.isTransaction) db.exec("ROLLBACK");
    throw new Error(`${label}: ${(error as Error).message}`);
  }
  record();
}

/**
 * Runs `steps` with the same connection settings storage.rs uses: foreign
 * keys off (so table rebuilds can't cascade), legacy_alter_table on (so a
 * rebuild isn't blocked by another table's trigger), both restored after.
 */
export function withMigrationSettings(db: DatabaseSync, steps: () => void): void {
  const foreignKeys = flag(db, "foreign_keys");
  const legacy = flag(db, "legacy_alter_table");
  db.exec("PRAGMA foreign_keys = OFF");
  try {
    steps();
  } finally {
    db.exec(`PRAGMA legacy_alter_table = ${legacy ? "ON" : "OFF"}`);
    db.exec(`PRAGMA foreign_keys = ${foreignKeys ? "ON" : "OFF"}`);
  }
}

export function applyUp(db: DatabaseSync, migration: Migration): void {
  runStep(db, `migration ${migration.version}`, migration.sql, () => {
    db.prepare("INSERT INTO _chain_migrations (version, name, checksum) VALUES (?, ?, ?)").run(
      migration.version,
      migration.name ?? null,
      checksum(migration.sql)
    );
  });
}

export function applyDown(db: DatabaseSync, migration: Migration): void {
  if (!migration.down) throw new Error(`migration ${migration.version} has no down script`);
  runStep(db, `reverting migration ${migration.version}`, migration.down, () => {
    db.prepare("DELETE FROM _chain_migrations WHERE version = ?").run(migration.version);
  });
}
