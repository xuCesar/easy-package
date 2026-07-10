use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use tauri::{AppHandle, Emitter, State};

use crate::{
    error::AppError,
    models::{
        EnvironmentScan, HealthIssue, ManagedPackage, ProjectMetadata, ScanProgress, TaskLog,
    },
    scan,
    storage::Storage,
};

#[derive(Clone, Default)]
pub struct ScanRegistry {
    active: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl ScanRegistry {
    fn begin(&self, scan_id: &str) -> Result<Arc<AtomicBool>, AppError> {
        let mut active = self
            .active
            .lock()
            .map_err(|error| AppError::Command(error.to_string()))?;
        if active.contains_key(scan_id) {
            return Err(AppError::Command("扫描会话已存在".into()));
        }
        let cancellation = Arc::new(AtomicBool::new(false));
        active.insert(scan_id.into(), cancellation.clone());
        Ok(cancellation)
    }

    fn finish(&self, scan_id: &str) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(scan_id);
        }
    }

    fn cancel(&self, scan_id: &str) {
        if let Ok(active) = self.active.lock() {
            if let Some(cancellation) = active.get(scan_id) {
                cancellation.store(true, Ordering::SeqCst);
            }
        }
    }
}

#[tauri::command]
pub async fn scan_environment(
    scan_id: String,
    app: AppHandle,
    storage: State<'_, Storage>,
    registry: State<'_, ScanRegistry>,
) -> Result<EnvironmentScan, AppError> {
    let cancellation = registry.begin(&scan_id)?;
    let registry = registry.inner().clone();
    let storage = storage.inner().clone();
    let scan_id_for_finish = scan_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let scan_id_for_progress = scan_id.clone();
        let progress = Arc::new(move |progress: ScanProgress| {
            let _ = app.emit("scan-progress", progress);
        });
        let scan =
            scan::scan_environment(&storage, &cancellation, &scan_id_for_progress, progress)?;
        storage.save_snapshot(&scan)?;
        Ok(scan)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()));
    registry.finish(&scan_id_for_finish);
    result?
}

#[tauri::command]
pub fn cancel_environment_scan(scan_id: String, registry: State<'_, ScanRegistry>) {
    registry.cancel(&scan_id);
}

#[tauri::command]
pub fn list_packages(storage: State<'_, Storage>) -> Result<Vec<ManagedPackage>, AppError> {
    Ok(storage
        .latest_snapshot()?
        .map(|snapshot| snapshot.packages)
        .unwrap_or_default())
}

#[tauri::command]
pub fn list_projects(storage: State<'_, Storage>) -> Result<Vec<ProjectMetadata>, AppError> {
    scan::projects_for_roots(storage.inner())
}

#[tauri::command]
pub async fn add_scan_root(
    path: String,
    storage: State<'_, Storage>,
) -> Result<Vec<ProjectMetadata>, AppError> {
    let path = PathBuf::from(path);
    if !path.is_dir() {
        return Err(AppError::InvalidScanRoot(
            path.to_string_lossy().into_owned(),
        ));
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| AppError::InvalidScanRoot(error.to_string()))?;
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        storage.add_scan_root(&canonical)?;
        scan::projects_for_roots(&storage)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub async fn remove_scan_root(
    path: String,
    storage: State<'_, Storage>,
) -> Result<Vec<ProjectMetadata>, AppError> {
    let path = PathBuf::from(path);
    let normalized = if path.exists() {
        path.canonicalize().unwrap_or(path)
    } else {
        path
    };
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        storage.remove_scan_root(&normalized)?;
        scan::projects_for_roots(&storage)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub fn get_health_report(storage: State<'_, Storage>) -> Result<Vec<HealthIssue>, AppError> {
    Ok(storage
        .latest_snapshot()?
        .map(|snapshot| snapshot.health_issues)
        .unwrap_or_default())
}

#[tauri::command]
pub fn get_scan_logs(storage: State<'_, Storage>) -> Result<Vec<TaskLog>, AppError> {
    storage.list_logs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancels_registered_scan_session() {
        let registry = ScanRegistry::default();
        let cancellation = registry.begin("scan-1").unwrap();
        registry.cancel("scan-1");
        assert!(cancellation.load(Ordering::SeqCst));
        registry.finish("scan-1");
    }
}
