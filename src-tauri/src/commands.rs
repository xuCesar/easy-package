use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use chrono::Utc;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::services;
use crate::{
    actions::{self, catalog::CatalogRegistry, ActionRegistry},
    error::AppError,
    models::{
        ActionCapability, CatalogSearchResponse, EnvironmentScan, HealthIssue, ManagedPackage,
        ObservedActionOutcome, PackageAction, PackageActionAuditRecord, PackageActionPlan,
        PackageActionReconciliationResult, PackageActionResult, PackageActionStatus,
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
            return Err(AppError::ScanConflict);
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

/// 项目重扫（添加/移除目录、设置变更、列表刷新）与环境扫描、写操作互斥，
/// 并携带真实取消令牌，避免并发文件树遍历与 SQLite 写入。
fn begin_project_rescan(
    registry: &ScanRegistry,
    coordinator: &OperationCoordinator,
) -> Result<(String, Arc<AtomicBool>), AppError> {
    let operation_id = format!("project-rescan-{}", Uuid::new_v4());
    coordinator.begin_scan(&operation_id)?;
    match registry.begin(&operation_id) {
        Ok(cancellation) => Ok((operation_id, cancellation)),
        Err(error) => {
            coordinator.finish(&operation_id);
            Err(error)
        }
    }
}

fn finish_project_rescan(
    registry: &ScanRegistry,
    coordinator: &OperationCoordinator,
    operation_id: &str,
) {
    registry.finish(operation_id);
    coordinator.finish(operation_id);
}

#[tauri::command]
pub async fn get_latest_snapshot(
    storage: State<'_, Storage>,
) -> Result<Option<EnvironmentScan>, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || storage.latest_snapshot())
        .await
        .map_err(|error| AppError::Command(error.to_string()))?
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
        let started_at = std::time::Instant::now();
        let span = tracing::info_span!("scan_environment", scan_id = %scan_id_for_progress);
        let _entered = span.enter();
        let progress = Arc::new(move |progress: ScanProgress| {
            tracing::debug!(phase = ?progress.phase, completed = progress.completed, total = progress.total, "扫描阶段进度");
            let _ = app.emit("scan-progress", progress);
        });
        #[cfg(feature = "e2e")]
        if crate::e2e::is_enabled() {
            return crate::e2e::scan(&cancellation, &scan_id_for_progress, progress.as_ref());
        }
        let scan =
            scan::scan_environment(&storage, &cancellation, &scan_id_for_progress, progress)?;
        storage.save_snapshot(&scan)?;
        storage.recover_incomplete_actions()?;
        if let Some(summary) = storage.list_snapshot_summaries()?.into_iter().next() {
            actions::reconcile_pending_audits(&storage, summary.id, &scan)?;
        }
        tracing::info!(
            duration_ms = started_at.elapsed().as_millis() as u64,
            managers = scan.managers.len(),
            projects = scan.projects.len(),
            partial_failures = scan.partial_failures,
            "环境扫描完成"
        );
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
pub async fn search_package_catalog(
    search_id: String,
    manager_id: crate::models::PackageManagerId,
    query: String,
    storage: State<'_, Storage>,
    registry: State<'_, CatalogRegistry>,
) -> Result<CatalogSearchResponse, AppError> {
    let cancellation = registry.begin(&search_id)?;
    let storage = storage.inner().clone();
    let registry = registry.inner().clone();
    let registry_for_search = registry.clone();
    let search_id_for_finish = search_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        actions::catalog::search(
            &storage,
            &registry_for_search,
            search_id,
            manager_id,
            query,
            &cancellation,
        )
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()));
    registry.finish(&search_id_for_finish);
    result?
}

#[tauri::command]
pub fn cancel_package_catalog_search(search_id: String, registry: State<'_, CatalogRegistry>) {
    registry.cancel(&search_id);
}

#[tauri::command]
pub async fn list_packages(storage: State<'_, Storage>) -> Result<Vec<ManagedPackage>, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(storage
            .latest_snapshot()?
            .map(|snapshot| snapshot.packages)
            .unwrap_or_default())
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub async fn list_projects(
    storage: State<'_, Storage>,
    registry: State<'_, ScanRegistry>,
    coordinator: State<'_, OperationCoordinator>,
) -> Result<Vec<ProjectMetadata>, AppError> {
    let (operation_id, cancellation) = begin_project_rescan(&registry, &coordinator)?;
    let storage = storage.inner().clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        Ok(scan::projects_for_roots(&storage, &cancellation)?.projects)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()));
    finish_project_rescan(&registry, &coordinator, &operation_id);
    result?
}

#[tauri::command]
pub async fn add_scan_root(
    path: String,
    storage: State<'_, Storage>,
    registry: State<'_, ScanRegistry>,
    coordinator: State<'_, OperationCoordinator>,
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
    let (operation_id, cancellation) = begin_project_rescan(&registry, &coordinator)?;
    let storage = storage.inner().clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        storage.add_scan_root(&canonical)?;
        scan::projects_for_roots(&storage, &cancellation)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()));
    finish_project_rescan(&registry, &coordinator, &operation_id);
    result?
}

#[tauri::command]
pub async fn remove_scan_root(
    path: String,
    storage: State<'_, Storage>,
    registry: State<'_, ScanRegistry>,
    coordinator: State<'_, OperationCoordinator>,
) -> Result<ProjectAnalysis, AppError> {
    let path = PathBuf::from(path);
    let normalized = if path.exists() {
        path.canonicalize().unwrap_or(path)
    } else {
        path
    };
    let (operation_id, cancellation) = begin_project_rescan(&registry, &coordinator)?;
    let storage = storage.inner().clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        storage.remove_scan_root(&normalized)?;
        let roots = storage.list_scan_roots()?;
        let mut settings = storage.scan_settings()?;
        settings.ignored_paths.retain(|ignored| {
            let ignored = PathBuf::from(ignored);
            roots.iter().any(|root| ignored.starts_with(root))
        });
        storage.save_scan_settings(&settings)?;
        scan::projects_for_roots(&storage, &cancellation)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()));
    finish_project_rescan(&registry, &coordinator, &operation_id);
    result?
}

#[tauri::command]
pub fn get_scan_settings(storage: State<'_, Storage>) -> Result<ScanSettings, AppError> {
    storage.scan_settings()
}

#[tauri::command]
pub async fn update_scan_settings(
    settings: ScanSettings,
    storage: State<'_, Storage>,
    registry: State<'_, ScanRegistry>,
    coordinator: State<'_, OperationCoordinator>,
) -> Result<ProjectAnalysis, AppError> {
    let (operation_id, cancellation) = begin_project_rescan(&registry, &coordinator)?;
    let storage = storage.inner().clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let roots = storage.list_scan_roots()?;
        let settings = scan::projects::normalize_scan_settings(settings, &roots)?;
        storage.save_scan_settings(&settings)?;
        scan::projects_for_roots(&storage, &cancellation)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()));
    finish_project_rescan(&registry, &coordinator, &operation_id);
    result?
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
        services::save_report_with_dialog(&app, format, format.file_name(), content)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub async fn list_snapshot_summaries(
    storage: State<'_, Storage>,
) -> Result<Vec<crate::models::SnapshotSummary>, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || storage.list_snapshot_summaries())
        .await
        .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub async fn compare_snapshots(
    baseline_id: i64,
    current_id: i64,
    storage: State<'_, Storage>,
) -> Result<crate::models::SnapshotComparison, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        services::snapshot_comparison(&storage, baseline_id, current_id)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
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
        let comparison = services::snapshot_comparison(&storage, baseline_id, current_id)?;
        let content = scan::report::build_comparison_report(&comparison, format)?;
        let file_name = match format {
            scan::report::ReportFormat::Json => "easy-package-changes.json",
            scan::report::ReportFormat::Markdown => "easy-package-changes.md",
        };
        services::save_report_with_dialog(&app, format, file_name, content)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub async fn get_health_report(storage: State<'_, Storage>) -> Result<Vec<HealthIssue>, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(storage
            .latest_snapshot()?
            .map(|snapshot| snapshot.health_issues)
            .unwrap_or_default())
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub async fn get_scan_logs(storage: State<'_, Storage>) -> Result<Vec<TaskLog>, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || storage.list_logs())
        .await
        .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub async fn plan_package_action(
    manager_id: crate::models::PackageManagerId,
    action: PackageAction,
    targets: Vec<String>,
    storage: State<'_, Storage>,
    registry: State<'_, ActionRegistry>,
) -> Result<PackageActionPlan, AppError> {
    let storage = storage.inner().clone();
    let registry = registry.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        actions::create_package_plan(&storage, &registry, manager_id, action, targets)
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub async fn get_package_action_capabilities(
    storage: State<'_, Storage>,
) -> Result<Vec<ActionCapability>, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || actions::package_action_capabilities(&storage))
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
        services::run_confirmed_package_action(
            &app,
            &storage,
            &registered,
            &cancellation,
            &action_id,
        )
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
pub async fn list_package_action_audit(
    storage: State<'_, Storage>,
) -> Result<Vec<PackageActionAuditRecord>, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || storage.list_action_audit())
        .await
        .map_err(|error| AppError::Command(error.to_string()))?
}

#[tauri::command]
pub async fn reconcile_package_action(
    action_id: String,
    storage: State<'_, Storage>,
    coordinator: State<'_, OperationCoordinator>,
) -> Result<PackageActionReconciliationResult, AppError> {
    let operation_id = format!("reconcile-{action_id}");
    coordinator.begin_scan(&operation_id)?;
    let coordinator = coordinator.inner().clone();
    let storage = storage.inner().clone();
    let operation_id_for_finish = operation_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut audit = storage
            .action_audit(&action_id)?
            .ok_or_else(|| AppError::Command("操作审计记录不存在".into()))?;
        let baseline = match audit.baseline_snapshot_id {
            Some(baseline_id) => storage.snapshot_by_id(baseline_id)?,
            None => None,
        };
        let (current, environment) =
            services::rescan_after_package_action(&storage, &operation_id)?;
        let (outcome, evidence) = match baseline.as_ref() {
            Some(baseline) => actions::reconcile_observed_outcome(
                audit.manager_id,
                audit.action,
                &audit.targets,
                baseline,
                &environment,
            ),
            None => (
                ObservedActionOutcome::Ambiguous,
                vec!["基线快照不存在或已过期，只能确认已完成最新扫描。".into()],
            ),
        };
        audit.result_snapshot_id = Some(current.id);
        audit.observed_outcome = Some(outcome);
        audit.evidence = evidence;
        audit.reconciled_at = Some(Utc::now().to_rfc3339());
        audit.rescan_required = false;
        if audit.status == PackageActionStatus::Running {
            audit.status = PackageActionStatus::Unknown;
            audit.error = Some("应用在操作完成前退出；已根据最新扫描核对实际环境。".into());
            audit.finished_at = Utc::now().to_rfc3339();
        }
        storage.save_action_audit(&audit)?;
        Ok(PackageActionReconciliationResult { audit, environment })
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()));
    coordinator.finish(&operation_id_for_finish);
    result?
}

#[tauri::command]
pub async fn get_project_dependency_graph(
    project_path: String,
    storage: State<'_, Storage>,
) -> Result<ProjectDependencyGraph, AppError> {
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        services::build_project_dependency_graph_for_path(&storage, &project_path)
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
            services::build_project_and_dependency_graph_for_path(&storage, &project_path)?;
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
            services::build_project_and_dependency_graph_for_path(&storage, &project_path)?;
        let risk_report = scan::supply_chain::build_supply_chain_report(&project, &graph);
        let content = scan::dependency_graph::build_cyclonedx_sbom_with_risk_summary(
            &graph,
            Some(&risk_report.summary),
        )?;
        services::save_report_with_dialog(
            &app,
            scan::report::ReportFormat::Json,
            "easy-package-sbom.cdx.json",
            content,
        )
    })
    .await
    .map_err(|error| AppError::Command(error.to_string()))?
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn project_rescan_is_mutually_exclusive_with_scans_and_actions() {
        let registry = ScanRegistry::default();
        let coordinator = OperationCoordinator::default();
        let (operation_id, cancellation) = begin_project_rescan(&registry, &coordinator).unwrap();
        assert!(!cancellation.load(Ordering::SeqCst));

        assert_eq!(
            registry.begin("scan-x").unwrap_err().code(),
            "SCAN_ALREADY_RUNNING"
        );
        assert_eq!(
            coordinator
                .begin_package_action("action-x")
                .unwrap_err()
                .code(),
            "SCAN_ALREADY_RUNNING"
        );

        finish_project_rescan(&registry, &coordinator, &operation_id);
        assert!(registry.begin("scan-x").is_ok());
        registry.finish("scan-x");
    }

    #[test]
    fn running_scan_blocks_project_rescans() {
        let registry = ScanRegistry::default();
        let coordinator = OperationCoordinator::default();
        coordinator.begin_scan("scan-1").unwrap();
        registry.begin("scan-1").unwrap();

        assert_eq!(
            begin_project_rescan(&registry, &coordinator)
                .unwrap_err()
                .code(),
            "SCAN_ALREADY_RUNNING"
        );

        registry.finish("scan-1");
        coordinator.finish("scan-1");
        let (operation_id, _cancellation) = begin_project_rescan(&registry, &coordinator).unwrap();
        finish_project_rescan(&registry, &coordinator, &operation_id);
    }

    #[test]
    fn projects_for_roots_honors_external_cancellation() {
        let directory = tempdir().unwrap();
        let storage = Storage::at(directory.path().join("test.sqlite3")).unwrap();
        let root = directory.path().join("code");
        fs::create_dir_all(root.join("app")).unwrap();
        fs::write(root.join("app/package.json"), "{\"name\":\"app\"}").unwrap();
        storage.add_scan_root(&root).unwrap();

        let cancelled = AtomicBool::new(true);
        let error = scan::projects_for_roots(&storage, &cancelled).unwrap_err();
        assert!(matches!(error, AppError::ScanCancelled));

        let active = AtomicBool::new(false);
        assert!(scan::projects_for_roots(&storage, &active).is_ok());
    }

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
        let error = registry.begin("scan-2").unwrap_err();
        assert_eq!(error.code(), "SCAN_ALREADY_RUNNING");
        registry.finish("scan-1");
        assert!(registry.begin("scan-2").is_ok());
    }

    #[test]
    fn rejects_identical_or_missing_snapshot_ids() {
        let directory = tempdir().unwrap();
        let storage = Storage::at(directory.path().join("test.sqlite3")).unwrap();

        assert!(services::snapshot_comparison(&storage, 1, 1)
            .unwrap_err()
            .to_string()
            .contains("两个不同的快照"));
        assert!(services::snapshot_comparison(&storage, 1, 2)
            .unwrap_err()
            .to_string()
            .contains("基线快照不存在或已被清理"));
        assert!(services::snapshot_comparison(&storage, 2, 1)
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

        let outside_error = services::build_project_dependency_graph_for_path(
            &storage,
            outside.to_string_lossy().as_ref(),
        )
        .unwrap_err()
        .to_string();
        assert!(outside_error.contains("必须位于已添加的扫描目录内"));

        let non_project_error = services::build_project_dependency_graph_for_path(
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
