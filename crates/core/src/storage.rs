//! Storage capability — see /agent-docs/capabilities/storage/CONTRACT.md.
//! SQLite is a portable C library (vendored via rusqlite's "bundled"
//! feature), so this implementation is identical on every desktop
//! platform — there is no per-OS branching here. What differs per OS is
//! only where the caller resolves the database file path to (handled by
//! Tauri's app_data_dir(), not by this crate).

use rusqlite::Connection;
use rusqlite::types::Value as SqlValue;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

#[derive(Debug)]
pub struct StorageError(pub String);

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Deserialize)]
pub struct Migration {
    pub version: i64,
    pub sql: String,
    /// Recorded in `_chain_migrations` for history. Generated migrations
    /// always carry one; hand-written ones may not. A `down` field, if
    /// present, is ignored here — only the CLI's `chain database update
    /// <target>` ever migrates down.
    #[serde(default)]
    pub name: Option<String>,
}

/// FNV-1a 64-bit of the migration's `sql`, hex — recorded when a migration
/// is applied so `chain migration list` can flag one edited afterwards.
/// packages/cli/src/schema/history.ts computes the identical value.
pub fn migration_checksum(sql: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in sql.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteResult {
    pub rows_affected: usize,
    pub last_insert_id: i64,
}

/// One SQLite connection guarded by a mutex — rusqlite::Connection isn't
/// Sync, and Tauri commands can run concurrently across threads.
pub struct Database(Mutex<Session>);

/// The connection plus the transaction open on it, if any. A transaction
/// spans several calls, so each call says which one it belongs to; a call
/// without the open transaction's id is refused rather than joining it.
struct Session {
    conn: Connection,
    transaction: Option<u64>,
    last_transaction: u64,
}

impl Database {
    /// Opens (creating if missing) the database file at `path`, creating
    /// its parent directory too, enables WAL journal mode, and ensures
    /// the internal migrations-tracking table exists.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| StorageError(e.to_string()))?;
        }
        let conn = Connection::open(path).map_err(|e| StorageError(e.to_string()))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| StorageError(e.to_string()))?;
        ensure_history_table(&conn)?;
        Ok(Self(Mutex::new(Session { conn, transaction: None, last_transaction: 0 })))
    }

    /// Locks the connection for a call made in `transaction`, or outside
    /// any transaction when `None`.
    fn session(&self, transaction: Option<u64>) -> Result<MutexGuard<'_, Session>, StorageError> {
        let session = self.0.lock().expect("storage mutex poisoned");
        match (transaction, session.transaction) {
            (None, Some(_)) => Err(StorageError("another transaction is open on this database".into())),
            (Some(id), open) if open != Some(id) => {
                Err(StorageError(format!("transaction {id} is no longer open")))
            }
            _ => Ok(session),
        }
    }

    /// Starts a transaction and returns its id, which every call inside it
    /// passes. IMMEDIATE takes the write lock now, so the transaction can't
    /// fail part-way on another connection holding it.
    pub fn begin(&self) -> Result<u64, StorageError> {
        let mut session = self.session(None)?;
        session.conn.execute_batch("BEGIN IMMEDIATE").map_err(|e| StorageError(e.to_string()))?;
        session.last_transaction += 1;
        session.transaction = Some(session.last_transaction);
        Ok(session.last_transaction)
    }

    /// A failed COMMIT is rolled back, so the transaction is closed either way.
    pub fn commit(&self, transaction: u64) -> Result<(), StorageError> {
        let mut session = self.session(Some(transaction))?;
        session.transaction = None;
        session.conn.execute_batch("COMMIT").map_err(|e| {
            if !session.conn.is_autocommit() {
                let _ = session.conn.execute_batch("ROLLBACK");
            }
            StorageError(e.to_string())
        })
    }

    /// Rolling back a transaction that's already closed is a no-op.
    pub fn rollback(&self, transaction: u64) -> Result<(), StorageError> {
        let mut session = self.0.lock().expect("storage mutex poisoned");
        if session.transaction != Some(transaction) {
            return Ok(());
        }
        rollback_open(&mut session)
    }

    /// Rolls back whatever transaction is open — for when the page that
    /// opened it reloads, and can never finish it.
    pub fn rollback_abandoned(&self) -> Result<(), StorageError> {
        rollback_open(&mut self.0.lock().expect("storage mutex poisoned"))
    }

    /// Runs every migration not yet recorded in `_chain_migrations`, in
    /// ascending version order, each as one batch. Safe to call
    /// repeatedly with a growing migration list.
    pub fn migrate(&self, migrations: &[Migration]) -> Result<(), StorageError> {
        let session = self.session(None)?;
        let conn = &session.conn;
        let applied: Vec<i64> = {
            let mut stmt = conn
                .prepare("SELECT version FROM _chain_migrations")
                .map_err(|e| StorageError(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| row.get::<_, i64>(0))
                .map_err(|e| StorageError(e.to_string()))?;
            rows.collect::<Result<_, _>>()
                .map_err(|e| StorageError(e.to_string()))?
        };

        let mut sorted: Vec<&Migration> = migrations.iter().filter(|m| !applied.contains(&m.version)).collect();
        if sorted.is_empty() {
            return Ok(());
        }
        sorted.sort_by_key(|m| m.version);

        // Migrations rebuild tables (create new, copy, drop old, rename), the
        // procedure in https://www.sqlite.org/lang_altertable.html. Two
        // connection settings get in its way, so both are switched for the
        // duration and restored afterwards:
        // - foreign_keys: the DROP would fire ON DELETE actions in child tables.
        // - legacy_alter_table: off, SQLite (3.26+) re-validates every trigger
        //   in the database on RENAME, and fails on one that mentions a table
        //   that's mid-rebuild. mneme's hand-written migration 6 hits exactly
        //   this on a fresh database.
        let foreign_keys = pragma_flag(&conn, "foreign_keys")?;
        let legacy_alter_table = pragma_flag(&conn, "legacy_alter_table")?;
        set_pragma(&conn, "foreign_keys", false)?;
        let result = sorted.iter().try_for_each(|m| {
            set_pragma(&conn, "legacy_alter_table", true)?;
            apply(&conn, m)
        });
        set_pragma(&conn, "legacy_alter_table", legacy_alter_table)?;
        set_pragma(&conn, "foreign_keys", foreign_keys)?;
        result
    }

    pub fn query(&self, transaction: Option<u64>, sql: &str, params: &[JsonValue]) -> Result<Vec<JsonValue>, StorageError> {
        let session = self.session(transaction)?;
        let mut stmt = session.conn.prepare(sql).map_err(|e| StorageError(e.to_string()))?;
        let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let sql_params: Vec<SqlValue> = params.iter().map(json_to_sql).collect();
        let param_refs: Vec<&dyn rusqlite::ToSql> =
            sql_params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                let mut obj = serde_json::Map::new();
                for (i, name) in column_names.iter().enumerate() {
                    let value: SqlValue = row.get(i)?;
                    obj.insert(name.clone(), sql_to_json(value));
                }
                Ok(JsonValue::Object(obj))
            })
            .map_err(|e| StorageError(e.to_string()))?;

        rows.collect::<Result<Vec<_>, _>>().map_err(|e| StorageError(e.to_string()))
    }

    pub fn execute(&self, transaction: Option<u64>, sql: &str, params: &[JsonValue]) -> Result<ExecuteResult, StorageError> {
        let session = self.session(transaction)?;
        let conn = &session.conn;
        let sql_params: Vec<SqlValue> = params.iter().map(json_to_sql).collect();
        let param_refs: Vec<&dyn rusqlite::ToSql> =
            sql_params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let rows_affected = conn
            .execute(sql, param_refs.as_slice())
            .map_err(|e| StorageError(e.to_string()))?;
        Ok(ExecuteResult {
            rows_affected,
            last_insert_id: conn.last_insert_rowid(),
        })
    }
}

/// Creates `_chain_migrations`, or adds the `name`/`checksum` columns to
/// one created before they existed. Mirrored by the CLI (history.ts).
fn ensure_history_table(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _chain_migrations (\
            version INTEGER PRIMARY KEY, \
            applied_at TEXT NOT NULL DEFAULT (datetime('now')), \
            name TEXT, \
            checksum TEXT\
        )",
    )
    .map_err(|e| StorageError(e.to_string()))?;
    let columns: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM pragma_table_info('_chain_migrations')")
            .map_err(|e| StorageError(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| StorageError(e.to_string()))?;
        rows.collect::<Result<_, _>>().map_err(|e| StorageError(e.to_string()))?
    };
    for column in ["name", "checksum"] {
        if !columns.iter().any(|c| c == column) {
            conn.execute_batch(&format!("ALTER TABLE _chain_migrations ADD COLUMN {column} TEXT"))
                .map_err(|e| StorageError(e.to_string()))?;
        }
    }
    Ok(())
}

fn rollback_open(session: &mut Session) -> Result<(), StorageError> {
    if session.transaction.take().is_none() || session.conn.is_autocommit() {
        return Ok(());
    }
    session.conn.execute_batch("ROLLBACK").map_err(|e| StorageError(e.to_string()))
}

fn pragma_flag(conn: &Connection, name: &str) -> Result<bool, StorageError> {
    conn.query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))
        .map_err(|e| StorageError(e.to_string()))
}

fn set_pragma(conn: &Connection, name: &str, on: bool) -> Result<(), StorageError> {
    conn.execute_batch(&format!("PRAGMA {name} = {}", if on { "ON" } else { "OFF" }))
        .map_err(|e| StorageError(e.to_string()))
}

/// Runs one migration's SQL, then records it. A failure part-way through a
/// migration that opened its own transaction (generated ones do) rolls it
/// back, so the connection is never left inside an open transaction.
fn apply(conn: &Connection, m: &Migration) -> Result<(), StorageError> {
    if let Err(e) = conn.execute_batch(&m.sql) {
        if !conn.is_autocommit() {
            let _ = conn.execute_batch("ROLLBACK");
        }
        return Err(StorageError(format!("migration {}: {}", m.version, e)));
    }
    conn.execute(
        "INSERT INTO _chain_migrations (version, name, checksum) VALUES (?1, ?2, ?3)",
        rusqlite::params![m.version, m.name, migration_checksum(&m.sql)],
    )
    .map_err(|e| StorageError(e.to_string()))?;
    Ok(())
}

fn json_to_sql(v: &JsonValue) -> SqlValue {
    match v {
        JsonValue::Null => SqlValue::Null,
        JsonValue::Bool(b) => SqlValue::Integer(if *b { 1 } else { 0 }),
        JsonValue::Number(n) => n
            .as_i64()
            .map(SqlValue::Integer)
            .unwrap_or_else(|| SqlValue::Real(n.as_f64().unwrap_or(0.0))),
        JsonValue::String(s) => SqlValue::Text(s.clone()),
        // Arrays/objects have no natural SQLite column type — store as JSON text.
        other => SqlValue::Text(other.to_string()),
    }
}

fn sql_to_json(v: SqlValue) -> JsonValue {
    match v {
        SqlValue::Null => JsonValue::Null,
        SqlValue::Integer(i) => JsonValue::from(i),
        SqlValue::Real(f) => serde_json::Number::from_f64(f).map(JsonValue::Number).unwrap_or(JsonValue::Null),
        SqlValue::Text(s) => JsonValue::String(s),
        // TODO: blob columns aren't supported yet — no real app need for
        // them yet (rule 7). Add base64 encoding here when one shows up.
        SqlValue::Blob(_) => JsonValue::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(test: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("chain-storage-{test}-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn temp_db() -> Database {
        Database::open(&temp_path("round-trip")).expect("open should succeed")
    }

    fn migration(version: i64, sql: &str) -> Migration {
        Migration { version, sql: sql.into(), name: Some(format!("m{version}")) }
    }

    #[test]
    fn migrate_query_execute_round_trip() {
        let db = temp_db();
        db.migrate(&[migration(1, "CREATE TABLE notes (id INTEGER PRIMARY KEY, title TEXT NOT NULL)")])
            .expect("migration should succeed");

        // re-running the same migration must be a no-op, not an error
        db.migrate(&[migration(1, "CREATE TABLE notes (id INTEGER PRIMARY KEY, title TEXT NOT NULL)")])
            .expect("re-applying an already-applied migration should be a no-op");

        let result = db
            .execute(
                None,
                "INSERT INTO notes (title) VALUES (?1)",
                &[JsonValue::String("first note".into())],
            )
            .expect("insert should succeed");
        assert_eq!(result.rows_affected, 1);
        assert_eq!(result.last_insert_id, 1);

        let rows = db.query(None, "SELECT id, title FROM notes", &[]).expect("query should succeed");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["title"], JsonValue::String("first note".into()));
    }

    #[test]
    fn transaction_commits_or_rolls_back_and_refuses_outside_calls() {
        let db = Database::open(&temp_path("transaction")).unwrap();
        db.migrate(&[migration(1, "CREATE TABLE t (x TEXT)")]).unwrap();
        let count = |db: &Database| db.query(None, "SELECT COUNT(*) AS n FROM t", &[]).unwrap()[0]["n"].clone();

        let tx = db.begin().unwrap();
        db.execute(Some(tx), "INSERT INTO t VALUES ('a')", &[]).unwrap();
        let err = db.execute(None, "INSERT INTO t VALUES ('b')", &[]).unwrap_err();
        assert!(err.0.contains("another transaction is open"));
        assert!(db.begin().is_err(), "one transaction at a time");
        db.commit(tx).unwrap();
        assert_eq!(count(&db), JsonValue::from(1));
        assert!(db.execute(Some(tx), "INSERT INTO t VALUES ('c')", &[]).unwrap_err().0.contains("no longer open"));

        let tx = db.begin().unwrap();
        db.execute(Some(tx), "INSERT INTO t VALUES ('d')", &[]).unwrap();
        db.rollback(tx).unwrap();
        db.rollback(tx).expect("rolling back a closed transaction is a no-op");
        assert_eq!(count(&db), JsonValue::from(1));

        let tx = db.begin().unwrap();
        db.execute(Some(tx), "INSERT INTO t VALUES ('e')", &[]).unwrap();
        db.rollback_abandoned().unwrap();
        assert_eq!(count(&db), JsonValue::from(1));
    }

    #[test]
    fn insert_returning_reads_the_row_back_through_query() {
        let db = Database::open(&temp_path("returning")).unwrap();
        db.migrate(&[migration(1, "CREATE TABLE t (id INTEGER PRIMARY KEY, x TEXT, at TEXT DEFAULT 'now')")]).unwrap();
        let rows = db.query(None, "INSERT INTO t (x) VALUES (?) RETURNING *", &[JsonValue::from("a")]).unwrap();
        assert_eq!(rows[0]["id"], JsonValue::from(1));
        assert_eq!(rows[0]["at"], JsonValue::from("now"));
        assert_eq!(db.query(None, "SELECT * FROM t", &[]).unwrap().len(), 1);
    }

    #[test]
    fn rebuilds_a_table_that_another_tables_trigger_mentions() {
        // mneme's migration 6 shape: rebuilding `parent` while a trigger on
        // `child` mentions it. Fails with legacy_alter_table off.
        let db = Database::open(&temp_path("legacy-rename")).unwrap();
        db.migrate(&[
            migration(1, "CREATE TABLE parent (id INTEGER PRIMARY KEY);
                          CREATE TABLE child (parent_id INTEGER);
                          CREATE TRIGGER child_insert BEFORE INSERT ON child BEGIN
                            SELECT RAISE(ABORT, 'no parent') WHERE NOT EXISTS (SELECT 1 FROM parent WHERE id = NEW.parent_id);
                          END;"),
            migration(2, "CREATE TABLE parent_new (id INTEGER PRIMARY KEY, name TEXT);
                          INSERT INTO parent_new (id) SELECT id FROM parent;
                          DROP TABLE parent;
                          ALTER TABLE parent_new RENAME TO parent;"),
        ])
        .expect("rebuild should succeed");
        let legacy = db.query(None, "PRAGMA legacy_alter_table", &[]).unwrap();
        assert_eq!(legacy[0]["legacy_alter_table"], JsonValue::from(0), "restored afterwards");
    }

    #[test]
    fn checksum_matches_fnv1a_reference_vectors() {
        assert_eq!(migration_checksum(""), "cbf29ce484222325");
        assert_eq!(migration_checksum("a"), "af63dc4c8601ec8c");
    }

    #[test]
    fn records_name_and_checksum_and_upgrades_old_history_tables() {
        let path = temp_path("history");
        {
            // A database from before name/checksum existed.
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE _chain_migrations (version INTEGER PRIMARY KEY, \
                 applied_at TEXT NOT NULL DEFAULT (datetime('now')));
                 INSERT INTO _chain_migrations (version) VALUES (1);",
            )
            .unwrap();
        }
        let db = Database::open(&path).unwrap();
        db.migrate(&[migration(1, "SELECT 1"), migration(2, "CREATE TABLE t (x TEXT)")]).unwrap();
        let rows = db.query(None, "SELECT version, name, checksum FROM _chain_migrations ORDER BY version", &[]).unwrap();
        assert_eq!(rows[0]["name"], JsonValue::Null);
        assert_eq!(rows[1]["name"], JsonValue::String("m2".into()));
        assert_eq!(rows[1]["checksum"], JsonValue::String(migration_checksum("CREATE TABLE t (x TEXT)")));
    }

    #[test]
    fn failed_migration_rolls_back_and_restores_foreign_keys() {
        let db = Database::open(&temp_path("rollback")).unwrap();
        db.execute(None, "PRAGMA foreign_keys = ON", &[]).unwrap();
        let err = db
            .migrate(&[migration(1, "BEGIN; CREATE TABLE t (x TEXT); INSERT INTO missing VALUES (1); COMMIT;")])
            .unwrap_err();
        assert!(err.0.contains("migration 1"));
        // Rolled back: no table, no history row, not stuck in a transaction.
        assert!(db.query(None, "SELECT * FROM t", &[]).is_err());
        assert!(db.query(None, "SELECT * FROM _chain_migrations", &[]).unwrap().is_empty());
        db.execute(None, "INSERT INTO _chain_migrations (version) VALUES (99)", &[]).unwrap();
        let fk = db.query(None, "PRAGMA foreign_keys", &[]).unwrap();
        assert_eq!(fk[0]["foreign_keys"], JsonValue::from(1));
    }
}
