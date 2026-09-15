//! Storage capability — see /capabilities/storage/CONTRACT.md.
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
use std::sync::Mutex;

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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteResult {
    pub rows_affected: usize,
    pub last_insert_id: i64,
}

/// One SQLite connection guarded by a mutex — rusqlite::Connection isn't
/// Sync, and Tauri commands can run concurrently across threads.
pub struct Database(Mutex<Connection>);

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
        conn.execute(
            "CREATE TABLE IF NOT EXISTS _chain_migrations (\
                version INTEGER PRIMARY KEY, \
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))\
            )",
            [],
        )
        .map_err(|e| StorageError(e.to_string()))?;
        Ok(Self(Mutex::new(conn)))
    }

    /// Runs every migration not yet recorded in `_chain_migrations`, in
    /// ascending version order, each as one batch. Safe to call
    /// repeatedly with a growing migration list.
    pub fn migrate(&self, migrations: &[Migration]) -> Result<(), StorageError> {
        let conn = self.0.lock().expect("storage mutex poisoned");
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

        let mut sorted: Vec<&Migration> = migrations.iter().collect();
        sorted.sort_by_key(|m| m.version);

        for m in sorted {
            if applied.contains(&m.version) {
                continue;
            }
            conn.execute_batch(&m.sql)
                .map_err(|e| StorageError(format!("migration {}: {}", m.version, e)))?;
            conn.execute(
                "INSERT INTO _chain_migrations (version) VALUES (?1)",
                rusqlite::params![m.version],
            )
            .map_err(|e| StorageError(e.to_string()))?;
        }
        Ok(())
    }

    pub fn query(&self, sql: &str, params: &[JsonValue]) -> Result<Vec<JsonValue>, StorageError> {
        let conn = self.0.lock().expect("storage mutex poisoned");
        let mut stmt = conn.prepare(sql).map_err(|e| StorageError(e.to_string()))?;
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

    pub fn execute(&self, sql: &str, params: &[JsonValue]) -> Result<ExecuteResult, StorageError> {
        let conn = self.0.lock().expect("storage mutex poisoned");
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

    fn temp_db() -> Database {
        let path = std::env::temp_dir().join(format!("chain-storage-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Database::open(&path).expect("open should succeed")
    }

    #[test]
    fn migrate_query_execute_round_trip() {
        let db = temp_db();
        db.migrate(&[Migration {
            version: 1,
            sql: "CREATE TABLE notes (id INTEGER PRIMARY KEY, title TEXT NOT NULL)".into(),
        }])
        .expect("migration should succeed");

        // re-running the same migration must be a no-op, not an error
        db.migrate(&[Migration {
            version: 1,
            sql: "CREATE TABLE notes (id INTEGER PRIMARY KEY, title TEXT NOT NULL)".into(),
        }])
        .expect("re-applying an already-applied migration should be a no-op");

        let result = db
            .execute(
                "INSERT INTO notes (title) VALUES (?1)",
                &[JsonValue::String("first note".into())],
            )
            .expect("insert should succeed");
        assert_eq!(result.rows_affected, 1);
        assert_eq!(result.last_insert_id, 1);

        let rows = db.query("SELECT id, title FROM notes", &[]).expect("query should succeed");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["title"], JsonValue::String("first note".into()));
    }
}
