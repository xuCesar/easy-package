use std::path::PathBuf;

use tauri::State;

use crate::{
    error::AppError,
    models::{EnvironmentScan, HealthIssue, ManagedPackage, ProjectMetadata, TaskLog},
    scan,
    storage::Storage,
};

#[tauri::command]
pub async fn scan_environment(storage: State<'_, Storage>) -> Result<EnvironmentScan, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let scan = scan::scan_environment(&storage)?;
        storage.save_snapshot(&scan)?;
        Ok(scan)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
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
