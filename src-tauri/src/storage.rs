use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{params, Connection};
use tauri::{AppHandle, Manager};

use crate::{
    error::AppError,
    models::{EnvironmentScan, TaskLog},
};

#[derive(Debug, Clone)]
pub struct Storage {
    path: PathBuf,
}

impl Storage {
    pub fn new(app: &AppHandle) -> Result<Self, AppError> {
        let directory = app
            .path()
            .app_data_dir()
            .map_err(|error| AppError::Storage(error.to_string()))?;
        fs::create_dir_all(&directory).map_err(|error| AppError::Storage(error.to_string()))?;
        let storage = Self {
            path: directory.join("devpkg.sqlite3"),
        };
        storage.initialize()?;
        Ok(storage)
    }

    #[cfg(test)]
    pub fn at(path: PathBuf) -> Result<Self, AppError> {
        let storage = Self { path };
        storage.initialize()?;
        Ok(storage)
    }

    fn connection(&self) -> Result<Connection, AppError> {
        Ok(Connection::open(&self.path)?)
    }

    fn initialize(&self) -> Result<(), AppError> {
        self.connection()?.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS scan_roots (
               path TEXT PRIMARY KEY,
               created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS snapshots (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               scanned_at TEXT NOT NULL,
               payload TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS scan_logs (
               id TEXT PRIMARY KEY,
               timestamp TEXT NOT NULL,
               payload TEXT NOT NULL
             );",
        )?;
        Ok(())
    }

    pub fn add_scan_root(&self, path: &Path) -> Result<(), AppError> {
        let value = path.to_string_lossy();
        self.connection()?.execute(
            "INSERT OR IGNORE INTO scan_roots(path, created_at) VALUES (?1, ?2)",
            params![value.as_ref(), chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn remove_scan_root(&self, path: &Path) -> Result<(), AppError> {
        self.connection()?.execute(
            "DELETE FROM scan_roots WHERE path = ?1",
            params![path.to_string_lossy().as_ref()],
        )?;
        Ok(())
    }

    pub fn list_scan_roots(&self) -> Result<Vec<PathBuf>, AppError> {
        let connection = self.connection()?;
        let mut statement =
            connection.prepare("SELECT path FROM scan_roots ORDER BY created_at")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|row| row.map(PathBuf::from).map_err(AppError::from))
            .collect()
    }

    pub fn save_snapshot(&self, scan: &EnvironmentScan) -> Result<(), AppError> {
        let connection = self.connection()?;
        let payload = serde_json::to_string(scan)?;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO snapshots(scanned_at, payload) VALUES (?1, ?2)",
            params![scan.scanned_at, payload],
        )?;
        for log in &scan.logs {
            transaction.execute(
                "INSERT OR REPLACE INTO scan_logs(id, timestamp, payload) VALUES (?1, ?2, ?3)",
                params![log.id, log.timestamp, serde_json::to_string(log)?],
            )?;
        }
        transaction.execute(
            "DELETE FROM snapshots WHERE id NOT IN (SELECT id FROM snapshots ORDER BY id DESC LIMIT 10)",
            [],
        )?;
        transaction.execute(
            "DELETE FROM scan_logs WHERE id NOT IN (SELECT id FROM scan_logs ORDER BY timestamp DESC LIMIT 200)",
            [],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn latest_snapshot(&self) -> Result<Option<EnvironmentScan>, AppError> {
        let connection = self.connection()?;
        let mut statement =
            connection.prepare("SELECT payload FROM snapshots ORDER BY id DESC LIMIT 1")?;
        let mut rows = statement.query([])?;
        match rows.next()? {
            Some(row) => {
                let payload: String = row.get(0)?;
                Ok(Some(serde_json::from_str(&payload)?))
            }
            None => Ok(None),
        }
    }

    pub fn list_logs(&self) -> Result<Vec<TaskLog>, AppError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT payload FROM scan_logs ORDER BY timestamp DESC LIMIT 200")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|row| {
            let payload = row?;
            serde_json::from_str(&payload).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(AppError::from)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn stores_scan_roots_without_duplicates() {
        let directory = tempdir().unwrap();
        let storage = Storage::at(directory.path().join("test.sqlite3")).unwrap();
        storage.add_scan_root(directory.path()).unwrap();
        storage.add_scan_root(directory.path()).unwrap();
        assert_eq!(storage.list_scan_roots().unwrap(), vec![directory.path()]);
        storage.remove_scan_root(directory.path()).unwrap();
        assert!(storage.list_scan_roots().unwrap().is_empty());
    }
}
