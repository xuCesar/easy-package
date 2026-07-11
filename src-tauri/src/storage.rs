use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{params, Connection};
use tauri::{AppHandle, Manager};

use crate::{
    error::AppError,
    models::{EnvironmentScan, PackageActionAuditRecord, ScanSettings, SnapshotSummary, TaskLog},
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
             );
             CREATE TABLE IF NOT EXISTS scan_settings (
               key TEXT PRIMARY KEY,
               payload TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS package_action_audit (
               action_id TEXT PRIMARY KEY,
               finished_at TEXT NOT NULL,
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

    pub fn scan_settings(&self) -> Result<ScanSettings, AppError> {
        let connection = self.connection()?;
        let mut statement =
            connection.prepare("SELECT payload FROM scan_settings WHERE key = ?1")?;
        let mut rows = statement.query(["project-scan"])?;
        match rows.next()? {
            Some(row) => Ok(serde_json::from_str(&row.get::<_, String>(0)?)?),
            None => Ok(ScanSettings::default()),
        }
    }

    pub fn save_scan_settings(&self, settings: &ScanSettings) -> Result<(), AppError> {
        self.connection()?.execute(
            "INSERT INTO scan_settings(key, payload) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET payload = excluded.payload",
            params!["project-scan", serde_json::to_string(settings)?],
        )?;
        Ok(())
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

    pub fn list_snapshot_summaries(&self) -> Result<Vec<SnapshotSummary>, AppError> {
        let connection = self.connection()?;
        let mut statement =
            connection.prepare("SELECT id, payload FROM snapshots ORDER BY id DESC")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (id, payload) = row?;
            let scan = serde_json::from_str::<EnvironmentScan>(&payload).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(SnapshotSummary::from_scan(id, &scan))
        })
        .collect::<Result<Vec<_>, rusqlite::Error>>()
        .map_err(AppError::from)
    }

    pub fn snapshot_by_id(&self, id: i64) -> Result<Option<EnvironmentScan>, AppError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT payload FROM snapshots WHERE id = ?1")?;
        let mut rows = statement.query([id])?;
        match rows.next()? {
            Some(row) => Ok(Some(serde_json::from_str(&row.get::<_, String>(0)?)?)),
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

    pub fn save_action_audit(&self, record: &PackageActionAuditRecord) -> Result<(), AppError> {
        let connection = self.connection()?;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT OR REPLACE INTO package_action_audit(action_id, finished_at, payload) VALUES (?1, ?2, ?3)",
            params![record.action_id, record.finished_at, serde_json::to_string(record)?],
        )?;
        transaction.execute(
            "DELETE FROM package_action_audit WHERE action_id NOT IN (SELECT action_id FROM package_action_audit ORDER BY finished_at DESC LIMIT 100)",
            [],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_action_audit(&self) -> Result<Vec<PackageActionAuditRecord>, AppError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT payload FROM package_action_audit ORDER BY finished_at DESC LIMIT 100",
        )?;
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

    pub fn has_incomplete_action(&self) -> Result<bool, AppError> {
        Ok(self
            .list_action_audit()?
            .iter()
            .any(|record| record.status == crate::models::PackageActionStatus::Running))
    }

    pub fn recover_incomplete_actions(&self) -> Result<usize, AppError> {
        let mut recovered = 0;
        for mut record in self.list_action_audit()? {
            if record.status != crate::models::PackageActionStatus::Running {
                continue;
            }
            record.status = crate::models::PackageActionStatus::Unknown;
            record.error =
                Some("应用在操作完成前退出；已重新扫描环境，但无法确认中断前已完成的步骤。".into());
            record.finished_at = chrono::Utc::now().to_rfc3339();
            self.save_action_audit(&record)?;
            recovered += 1;
        }
        Ok(recovered)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::models::{
        PackageAction, PackageActionAuditRecord, PackageActionStatus, PackageManagerId,
    };

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

    #[test]
    fn stores_package_action_audit_records_without_raw_command_arguments() {
        let directory = tempdir().unwrap();
        let storage = Storage::at(directory.path().join("test.sqlite3")).unwrap();
        storage
            .save_action_audit(&PackageActionAuditRecord {
                action_id: "action-1".into(),
                plan_id: "plan-1".into(),
                manager_id: PackageManagerId::Homebrew,
                action: PackageAction::Install,
                targets: vec!["jq".into()],
                status: PackageActionStatus::Succeeded,
                command_preview: "/opt/homebrew/bin/brew install jq".into(),
                logs: vec!["安装完成".into()],
                error: None,
                started_at: "2026-07-11T00:00:00Z".into(),
                finished_at: "2026-07-11T00:00:01Z".into(),
            })
            .unwrap();
        let records = storage.list_action_audit().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].targets, vec!["jq"]);
    }

    #[test]
    fn recovers_incomplete_action_after_a_successful_scan_boundary() {
        let directory = tempdir().unwrap();
        let storage = Storage::at(directory.path().join("test.sqlite3")).unwrap();
        storage
            .save_action_audit(&PackageActionAuditRecord {
                action_id: "action-running".into(),
                plan_id: "plan-running".into(),
                manager_id: PackageManagerId::Pnpm,
                action: PackageAction::Upgrade,
                targets: vec!["typescript".into()],
                status: PackageActionStatus::Running,
                command_preview: "pnpm update --global --ignore-scripts typescript".into(),
                logs: vec![],
                error: None,
                started_at: "2026-07-11T00:00:00Z".into(),
                finished_at: "2026-07-11T00:00:00Z".into(),
            })
            .unwrap();
        assert!(storage.has_incomplete_action().unwrap());
        assert_eq!(storage.recover_incomplete_actions().unwrap(), 1);
        let record = storage.list_action_audit().unwrap().remove(0);
        assert_eq!(record.status, PackageActionStatus::Unknown);
        assert!(record.error.unwrap().contains("应用在操作完成前退出"));
        assert!(!storage.has_incomplete_action().unwrap());
    }

    #[test]
    fn reads_default_settings_when_an_existing_database_has_no_settings_row() {
        let directory = tempdir().unwrap();
        let storage = Storage::at(directory.path().join("test.sqlite3")).unwrap();
        assert_eq!(storage.scan_settings().unwrap(), ScanSettings::default());
    }

    #[test]
    fn reads_old_settings_payload_without_default_ignored_directory_names() {
        let directory = tempdir().unwrap();
        let storage = Storage::at(directory.path().join("test.sqlite3")).unwrap();
        storage
            .connection()
            .unwrap()
            .execute(
                "INSERT INTO scan_settings(key, payload) VALUES (?1, ?2)",
                params!["project-scan", r#"{"ignoredPaths":[],"maxDepth":4}"#],
            )
            .unwrap();

        let settings = storage.scan_settings().unwrap();
        assert_eq!(settings.max_depth, 4);
        assert!(settings
            .default_ignored_directory_names
            .contains(&"node_modules".into()));
    }

    #[test]
    fn lists_snapshot_summaries_and_reads_snapshots_by_id() {
        let directory = tempdir().unwrap();
        let storage = Storage::at(directory.path().join("test.sqlite3")).unwrap();
        let scan: EnvironmentScan = serde_json::from_str(
            r#"{"managers":[],"packages":[],"projects":[],"scanRoots":[],"healthIssues":[],"logs":[],"pathObservations":[],"scannedAt":"2026-01-01T00:00:00Z","partialFailures":0}"#,
        )
        .unwrap();
        storage.save_snapshot(&scan).unwrap();

        let summaries = storage.list_snapshot_summaries().unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].scanned_at, "2026-01-01T00:00:00Z");
        assert!(storage.snapshot_by_id(summaries[0].id).unwrap().is_some());
        assert!(storage.snapshot_by_id(999).unwrap().is_none());
    }
}
