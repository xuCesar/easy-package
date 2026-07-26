//! 命令层背后的业务编排：报告导出、快照比较与写操作执行流程。
use std::{fs, path::PathBuf, sync::atomic::AtomicBool, sync::Arc};

use chrono::Utc;
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;

use crate::{
    actions,
    error::AppError,
    models::{
        EnvironmentScan, PackageActionAuditRecord, PackageActionProgress, PackageActionResult,
        PackageActionStatus, ProjectDependencyGraph, ProjectMetadata,
    },
    scan,
    storage::Storage,
};

pub(crate) fn snapshot_comparison(
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

pub(crate) fn save_report_with_dialog(
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

pub(crate) fn rescan_after_package_action(
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
pub(crate) fn build_project_dependency_graph_for_path(
    storage: &Storage,
    project_path: &str,
) -> Result<ProjectDependencyGraph, AppError> {
    Ok(build_project_and_dependency_graph_for_path(storage, project_path)?.1)
}

pub(crate) fn build_project_and_dependency_graph_for_path(
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
    // 优先用最近快照定位项目，避免每次按需加载都触发全量项目扫描；
    // 锁文件内容由图构建时重新读取，不受快照新旧影响。
    let never_cancelled = AtomicBool::new(false);
    let snapshot_project = storage.latest_snapshot()?.and_then(|snapshot| {
        snapshot
            .projects
            .into_iter()
            .find(|project| PathBuf::from(&project.path) == canonical)
    });
    let project = match snapshot_project {
        Some(project) => project,
        None => scan::projects_for_roots(storage, &never_cancelled)?
            .projects
            .into_iter()
            .find(|project| PathBuf::from(&project.path) == canonical)
            .ok_or_else(|| AppError::InvalidScanRoot("该路径不是已识别项目".into()))?,
    };
    let graph =
        scan::dependency_graph::build_project_dependency_graph(&project, &AtomicBool::new(false))?;
    Ok((project, graph))
}

/// 已确认写操作的完整编排：审计基线、执行、复扫、结果比对与审计落库。
pub(crate) fn run_confirmed_package_action(
    app: &AppHandle,
    storage: &Storage,
    registered: &actions::RegisteredPlan,
    cancellation: &std::sync::atomic::AtomicBool,
    action_id: &str,
) -> Result<PackageActionResult, AppError> {
    let started_at = Utc::now().to_rfc3339();
    let baseline_summary = storage.list_snapshot_summaries()?.first().cloned();
    let baseline_scan = storage.latest_snapshot()?;
    storage.save_action_audit(&PackageActionAuditRecord {
        action_id: action_id.to_string(),
        plan_id: registered.plan.id.clone(),
        manager_id: registered.plan.manager_id,
        action: registered.plan.action,
        targets: registered.plan.targets.clone(),
        status: PackageActionStatus::Running,
        command_preview: registered.plan.command_preview.clone(),
        logs: Vec::new(),
        error: None,
        started_at: started_at.clone(),
        // 运行中记录复用现有排序字段；最终结果会原位覆盖该时间。
        finished_at: started_at.clone(),
        baseline_snapshot_id: baseline_summary.as_ref().map(|summary| summary.id),
        result_snapshot_id: None,
        observed_outcome: None,
        evidence: Vec::new(),
        reconciled_at: None,
        rescan_required: true,
    })?;
    emit_action_progress(
        app,
        &action_id,
        PackageActionStatus::Running,
        &format!(
            "开始执行已确认的 {} 操作",
            registered.plan.manager_id.as_str()
        ),
        true,
    );
    let progress_app = app.clone();
    let progress_action_id = action_id;
    let on_log = move |message: String| {
        emit_action_progress(
            &progress_app,
            &progress_action_id,
            PackageActionStatus::Running,
            &message,
            true,
        );
    };
    let execution = actions::execute_registered_plan(registered, cancellation, &on_log);
    emit_action_progress(
        app,
        &action_id,
        PackageActionStatus::Running,
        "正在重新扫描本机环境并计算变化",
        false,
    );
    let mut result_error = execution.error.clone();
    let mut environment = None;
    let mut result_snapshot_id = None;
    let mut observed_outcome = None;
    let mut evidence = Vec::new();
    let mut rescan_required = false;
    let comparison = match rescan_after_package_action(storage, action_id) {
        Ok((current, scan)) => {
            result_snapshot_id = Some(current.id);
            let comparison = match (baseline_summary.as_ref(), baseline_scan.as_ref()) {
                (Some(baseline), Some(baseline_scan)) => {
                    let observed = actions::reconcile_observed_outcome(
                        registered.plan.manager_id,
                        registered.plan.action,
                        &registered.plan.targets,
                        baseline_scan,
                        &scan,
                    );
                    observed_outcome = Some(observed.0);
                    evidence = observed.1;
                    scan::history::compare_snapshots(baseline.id, baseline_scan, current.id, &scan)
                        .ok()
                }
                _ => None,
            };
            environment = Some(scan);
            comparison
        }
        Err(error) => {
            append_error(&mut result_error, format!("操作后重新扫描失败：{error}"));
            rescan_required = true;
            None
        }
    };
    let finished_at = Utc::now().to_rfc3339();
    let mut result = PackageActionResult {
        action_id: action_id.to_string(),
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
        baseline_snapshot_id: baseline_summary.as_ref().map(|summary| summary.id),
        result_snapshot_id,
        observed_outcome,
        evidence,
        reconciled_at: (!rescan_required).then(|| Utc::now().to_rfc3339()),
        rescan_required,
    };
    if let Err(error) = storage.save_action_audit(&audit) {
        append_error(&mut result.error, format!("操作审计记录保存失败：{error}"));
    }
    emit_action_progress(
        app,
        &action_id,
        result.status,
        match result.status {
            PackageActionStatus::Succeeded => "操作完成，环境已重新扫描",
            PackageActionStatus::Failed => "包管理器操作失败，环境已重新扫描",
            PackageActionStatus::Unknown => "操作状态未知，环境已重新扫描",
            PackageActionStatus::Planned | PackageActionStatus::Running => {
                "操作结束，环境已重新扫描"
            }
        },
        false,
    );
    Ok(result)
}
