use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use crate::{
    error::AppError,
    models::{
        EnvironmentScan, HealthIssue, ManagedPackage, ProjectAnalysis, ProjectMetadata,
        ScanProgress, ScanSettings, TaskLog,
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
        if !active.is_empty() {
            return Err(AppError::Command(
                "SCAN_ALREADY_RUNNING：已有环境扫描正在进行".into(),
            ));
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
        #[cfg(feature = "e2e")]
        if crate::e2e::is_enabled() {
            return crate::e2e::scan(&cancellation, &scan_id_for_progress, progress.as_ref());
        }
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
    Ok(scan::projects_for_roots(storage.inner())?.projects)
}

#[tauri::command]
pub async fn add_scan_root(
    path: String,
    storage: State<'_, Storage>,
) -> Result<ProjectAnalysis, AppError> {
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
) -> Result<ProjectAnalysis, AppError> {
    let path = PathBuf::from(path);
    let normalized = if path.exists() {
        path.canonicalize().unwrap_or(path)
    } else {
        path
    };
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        storage.remove_scan_root(&normalized)?;
        let roots = storage.list_scan_roots()?;
        let mut settings = storage.scan_settings()?;
        settings.ignored_paths.retain(|ignored| {
            let ignored = PathBuf::from(ignored);
            roots.iter().any(|root| ignored.starts_with(root))
        });
        storage.save_scan_settings(&settings)?;
        scan::projects_for_roots(&storage)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub fn get_scan_settings(storage: State<'_, Storage>) -> Result<ScanSettings, AppError> {
    storage.scan_settings()
}

#[tauri::command]
pub async fn update_scan_settings(
    settings: ScanSettings,
    storage: State<'_, Storage>,
) -> Result<ProjectAnalysis, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let roots = storage.list_scan_roots()?;
        let settings = scan::projects::normalize_scan_settings(settings, &roots)?;
        storage.save_scan_settings(&settings)?;
        scan::projects_for_roots(&storage)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub async fn export_environment_report(
    format: scan::report::ReportFormat,
    app: AppHandle,
    storage: State<'_, Storage>,
) -> Result<scan::report::ReportExportResult, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let scan = storage
            .latest_snapshot()?
            .ok_or_else(|| AppError::Command("尚无可导出的环境扫描结果".into()))?;
        let content = scan::report::build_report(&scan, format)?;
        save_report_with_dialog(&app, format, format.file_name(), content)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub fn list_snapshot_summaries(
    storage: State<'_, Storage>,
) -> Result<Vec<crate::models::SnapshotSummary>, AppError> {
    storage.list_snapshot_summaries()
}

#[tauri::command]
pub fn compare_snapshots(
    baseline_id: i64,
    current_id: i64,
    storage: State<'_, Storage>,
) -> Result<crate::models::SnapshotComparison, AppError> {
    snapshot_comparison(storage.inner(), baseline_id, current_id)
}

#[tauri::command]
pub async fn export_snapshot_comparison_report(
    format: scan::report::ReportFormat,
    baseline_id: i64,
    current_id: i64,
    app: AppHandle,
    storage: State<'_, Storage>,
) -> Result<scan::report::ReportExportResult, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let comparison = snapshot_comparison(&storage, baseline_id, current_id)?;
        let content = scan::report::build_comparison_report(&comparison, format)?;
        let file_name = match format {
            scan::report::ReportFormat::Json => "easy-package-changes.json",
            scan::report::ReportFormat::Markdown => "easy-package-changes.md",
        };
        save_report_with_dialog(&app, format, file_name, content)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

fn snapshot_comparison(
    storage: &Storage,
    baseline_id: i64,
    current_id: i64,
) -> Result<crate::models::SnapshotComparison, AppError> {
    if baseline_id == current_id {
        return Err(AppError::Command("请选择两个不同的快照进行比较".into()));
    }
    if baseline_id > current_id {
        return Err(AppError::Command("基线快照必须早于当前快照".into()));
    }
    let baseline = storage
        .snapshot_by_id(baseline_id)?
        .ok_or_else(|| AppError::Command("基线快照不存在或已被清理".into()))?;
    let current = storage
        .snapshot_by_id(current_id)?
        .ok_or_else(|| AppError::Command("当前快照不存在或已被清理".into()))?;
    scan::history::compare_snapshots(baseline_id, &baseline, current_id, &current)
}

fn save_report_with_dialog(
    app: &AppHandle,
    format: scan::report::ReportFormat,
    file_name: &str,
    content: String,
) -> Result<scan::report::ReportExportResult, AppError> {
    let dialog = app
        .dialog()
        .file()
        .set_title("导出环境报告")
        .set_file_name(file_name)
        .add_filter(format.filter_name(), &[format.extension()]);
    let Some(file) = dialog.blocking_save_file() else {
        return Ok(scan::report::ReportExportResult {
            saved: false,
            path: None,
        });
    };
    let path = file
        .into_path()
        .map_err(|error| AppError::Command(error.to_string()))?;
    fs::write(path, content).map_err(|error| AppError::Command(error.to_string()))?;
    Ok(scan::report::ReportExportResult {
        saved: true,
        path: None,
    })
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
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn cancels_registered_scan_session() {
        let registry = ScanRegistry::default();
        let cancellation = registry.begin("scan-1").unwrap();
        registry.cancel("scan-1");
        assert!(cancellation.load(Ordering::SeqCst));
        registry.finish("scan-1");
    }

    #[test]
    fn rejects_a_second_concurrent_scan_session() {
        let registry = ScanRegistry::default();
        registry.begin("scan-1").unwrap();
        let error = registry.begin("scan-2").unwrap_err().to_string();
        assert!(error.contains("SCAN_ALREADY_RUNNING"));
        registry.finish("scan-1");
        assert!(registry.begin("scan-2").is_ok());
    }

    #[test]
    fn rejects_identical_or_missing_snapshot_ids() {
        let directory = tempdir().unwrap();
        let storage = Storage::at(directory.path().join("test.sqlite3")).unwrap();

        assert!(snapshot_comparison(&storage, 1, 1)
            .unwrap_err()
            .to_string()
            .contains("两个不同的快照"));
        assert!(snapshot_comparison(&storage, 1, 2)
            .unwrap_err()
            .to_string()
            .contains("基线快照不存在或已被清理"));
        assert!(snapshot_comparison(&storage, 2, 1)
            .unwrap_err()
            .to_string()
            .contains("基线快照必须早于当前快照"));
    }
}
