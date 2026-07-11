use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use chrono::Utc;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

use crate::{
    actions::{self, ActionRegistry},
    error::AppError,
    models::{
        EnvironmentScan, HealthIssue, ManagedPackage, PackageAction, PackageActionAuditRecord,
        PackageActionPlan, PackageActionProgress, PackageActionResult, PackageActionStatus,
        ProjectAnalysis, ProjectDependencyGraph, ProjectMetadata, ProjectSupplyChainReport,
        ScanProgress, ScanSettings, TaskLog,
    },
    operation_guard::OperationCoordinator,
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
    coordinator: State<'_, OperationCoordinator>,
) -> Result<EnvironmentScan, AppError> {
    coordinator.begin_scan(&scan_id)?;
    let coordinator = coordinator.inner().clone();
    let cancellation = match registry.begin(&scan_id) {
        Ok(cancellation) => cancellation,
        Err(error) => {
            coordinator.finish(&scan_id);
            return Err(error);
        }
    };
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
    coordinator.finish(&scan_id_for_finish);
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

#[tauri::command]
pub async fn plan_homebrew_action(
    action: PackageAction,
    targets: Vec<String>,
    storage: State<'_, Storage>,
    registry: State<'_, ActionRegistry>,
) -> Result<PackageActionPlan, AppError> {
    let storage = storage.inner().clone();
    let registry = registry.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        actions::create_homebrew_plan(&storage, &registry, action, targets)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub async fn execute_package_action(
    plan_id: String,
    app: AppHandle,
    storage: State<'_, Storage>,
    registry: State<'_, ActionRegistry>,
    coordinator: State<'_, OperationCoordinator>,
) -> Result<PackageActionResult, AppError> {
    let action_id = Uuid::new_v4().to_string();
    coordinator.begin_package_action(&action_id)?;
    let cancellation = match registry.begin(&action_id) {
        Ok(cancellation) => cancellation,
        Err(error) => {
            coordinator.finish(&action_id);
            return Err(error);
        }
    };
    let registered = match registry.take(&plan_id) {
        Ok(registered) => registered,
        Err(error) => {
            registry.finish(&action_id);
            coordinator.finish(&action_id);
            return Err(error);
        }
    };
    let storage = storage.inner().clone();
    let registry = registry.inner().clone();
    let coordinator = coordinator.inner().clone();
    let action_id_for_finish = action_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let started_at = Utc::now().to_rfc3339();
        let baseline_summary = storage.list_snapshot_summaries()?.first().cloned();
        let baseline_scan = storage.latest_snapshot()?;
        emit_action_progress(
            &app,
            &action_id,
            PackageActionStatus::Running,
            "开始执行已确认的 Homebrew 操作",
            true,
        );
        let progress_app = app.clone();
        let progress_action_id = action_id.clone();
        let on_log = move |message: String| {
            emit_action_progress(
                &progress_app,
                &progress_action_id,
                PackageActionStatus::Running,
                &message,
                true,
            );
        };
        let execution = actions::execute_registered_plan(&registered, &cancellation, &on_log);
        emit_action_progress(
            &app,
            &action_id,
            PackageActionStatus::Running,
            "正在重新扫描本机环境并计算变化",
            false,
        );
        let mut result_error = execution.error.clone();
        let mut environment = None;
        let comparison = match rescan_after_package_action(&storage, &action_id) {
            Ok((current, scan)) => {
                let comparison = match (baseline_summary.as_ref(), baseline_scan.as_ref()) {
                    (Some(baseline), Some(baseline_scan)) => scan::history::compare_snapshots(
                        baseline.id,
                        baseline_scan,
                        current.id,
                        &scan,
                    )
                    .ok(),
                    _ => None,
                };
                environment = Some(scan);
                comparison
            }
            Err(error) => {
                append_error(&mut result_error, format!("操作后重新扫描失败：{error}"));
                None
            }
        };
        let finished_at = Utc::now().to_rfc3339();
        let mut result = PackageActionResult {
            action_id: action_id.clone(),
            plan_id: registered.plan.id.clone(),
            manager_id: registered.plan.manager_id,
            action: registered.plan.action,
            targets: registered.plan.targets.clone(),
            status: execution.status,
            command_preview: registered.plan.command_preview.clone(),
            logs: execution.logs,
            error: result_error,
            comparison,
            environment,
            started_at,
            finished_at,
        };
        let audit = PackageActionAuditRecord {
            action_id: result.action_id.clone(),
            plan_id: result.plan_id.clone(),
            manager_id: result.manager_id,
            action: result.action,
            targets: result.targets.clone(),
            status: result.status,
            command_preview: result.command_preview.clone(),
            logs: result.logs.clone(),
            error: result.error.clone(),
            started_at: result.started_at.clone(),
            finished_at: result.finished_at.clone(),
        };
        if let Err(error) = storage.save_action_audit(&audit) {
            append_error(&mut result.error, format!("操作审计记录保存失败：{error}"));
        }
        emit_action_progress(
            &app,
            &action_id,
            result.status,
            match result.status {
                PackageActionStatus::Succeeded => "操作完成，环境已重新扫描",
                PackageActionStatus::Failed => "Homebrew 操作失败，环境已重新扫描",
                PackageActionStatus::Unknown => "操作状态未知，环境已重新扫描",
                PackageActionStatus::Planned | PackageActionStatus::Running => {
                    "操作结束，环境已重新扫描"
                }
            },
            false,
        );
        Ok::<_, AppError>(result)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()));
    registry.finish(&action_id_for_finish);
    coordinator.finish(&action_id_for_finish);
    result?
}

#[tauri::command]
pub fn cancel_package_action(action_id: String, registry: State<'_, ActionRegistry>) {
    registry.cancel(&action_id);
}

#[tauri::command]
pub fn list_package_action_audit(
    storage: State<'_, Storage>,
) -> Result<Vec<PackageActionAuditRecord>, AppError> {
    storage.list_action_audit()
}

fn emit_action_progress(
    app: &AppHandle,
    action_id: &str,
    status: PackageActionStatus,
    message: &str,
    cancellable: bool,
) {
    let _ = app.emit(
        "package-action-progress",
        PackageActionProgress {
            action_id: action_id.into(),
            status,
            message: message.into(),
            cancellable,
            timestamp: Utc::now().to_rfc3339(),
        },
    );
}

fn rescan_after_package_action(
    storage: &Storage,
    action_id: &str,
) -> Result<(crate::models::SnapshotSummary, EnvironmentScan), AppError> {
    let scan_id = format!("post-action-{action_id}");
    let scan =
        scan::scan_environment(storage, &AtomicBool::new(false), &scan_id, Arc::new(|_| {}))?;
    storage.save_snapshot(&scan)?;
    let summary = storage
        .list_snapshot_summaries()?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Storage("操作后快照未保存".into()))?;
    Ok((summary, scan))
}

fn append_error(target: &mut Option<String>, value: String) {
    match target {
        Some(current) => {
            current.push('；');
            current.push_str(&value);
        }
        None => *target = Some(value),
    }
}

#[tauri::command]
pub async fn get_project_dependency_graph(
    project_path: String,
    storage: State<'_, Storage>,
) -> Result<ProjectDependencyGraph, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        build_project_dependency_graph_for_path(&storage, &project_path)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub async fn get_project_supply_chain_report(
    project_path: String,
    storage: State<'_, Storage>,
) -> Result<ProjectSupplyChainReport, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (project, graph) =
            build_project_and_dependency_graph_for_path(&storage, &project_path)?;
        Ok(scan::supply_chain::build_supply_chain_report(
            &project, &graph,
        ))
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub async fn export_project_sbom(
    project_path: String,
    app: AppHandle,
    storage: State<'_, Storage>,
) -> Result<scan::report::ReportExportResult, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (project, graph) =
            build_project_and_dependency_graph_for_path(&storage, &project_path)?;
        let risk_report = scan::supply_chain::build_supply_chain_report(&project, &graph);
        let content = scan::dependency_graph::build_cyclonedx_sbom_with_risk_summary(
            &graph,
            Some(&risk_report.summary),
        )?;
        save_report_with_dialog(
            &app,
            scan::report::ReportFormat::Json,
            "easy-package-sbom.cdx.json",
            content,
        )
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

fn build_project_dependency_graph_for_path(
    storage: &Storage,
    project_path: &str,
) -> Result<ProjectDependencyGraph, AppError> {
    Ok(build_project_and_dependency_graph_for_path(storage, project_path)?.1)
}

fn build_project_and_dependency_graph_for_path(
    storage: &Storage,
    project_path: &str,
) -> Result<(ProjectMetadata, ProjectDependencyGraph), AppError> {
    let requested = PathBuf::from(project_path);
    let canonical = requested
        .canonicalize()
        .map_err(|error| AppError::InvalidScanRoot(error.to_string()))?;
    let roots = storage.list_scan_roots()?;
    if !roots.iter().any(|root| canonical.starts_with(root)) {
        return Err(AppError::InvalidScanRoot(
            "依赖图项目必须位于已添加的扫描目录内".into(),
        ));
    }
    let analysis = scan::projects_for_roots(storage)?;
    let project = analysis
        .projects
        .into_iter()
        .find(|project| PathBuf::from(&project.path) == canonical)
        .ok_or_else(|| AppError::InvalidScanRoot("该路径不是已识别项目".into()))?;
    let graph =
        scan::dependency_graph::build_project_dependency_graph(&project, &AtomicBool::new(false))?;
    Ok((project, graph))
}

#[cfg(test)]
mod tests {
    use std::fs;

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

    #[test]
    fn dependency_graph_commands_reject_paths_outside_roots_and_non_projects() {
        let directory = tempdir().unwrap();
        let root = directory.path().join("root");
        let outside = directory.path().join("outside");
        let not_project = root.join("notes");
        fs::create_dir_all(&not_project).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("package.json"), r#"{"name":"outside"}"#).unwrap();
        let storage = Storage::at(directory.path().join("test.sqlite3")).unwrap();
        storage
            .add_scan_root(&root.canonicalize().unwrap())
            .unwrap();

        let outside_error =
            build_project_dependency_graph_for_path(&storage, outside.to_string_lossy().as_ref())
                .unwrap_err()
                .to_string();
        assert!(outside_error.contains("必须位于已添加的扫描目录内"));

        let non_project_error = build_project_dependency_graph_for_path(
            &storage,
            not_project.to_string_lossy().as_ref(),
        )
        .unwrap_err()
        .to_string();
        assert!(
            non_project_error.contains("不是已识别项目"),
            "意外错误：{non_project_error}"
        );
    }
}
