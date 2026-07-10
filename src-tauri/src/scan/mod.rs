mod health;
mod projects;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use chrono::Utc;
use uuid::Uuid;

use crate::{
    adapters,
    error::AppError,
    models::{
        EnvironmentScan, LogCategory, LogStatus, PathObservation, ProjectAnalysis, ScanPhase,
        ScanProgress, TaskLog,
    },
    storage::Storage,
};

use crate::adapters::runner::find_all_in_path;
pub use projects::analyze_projects;

const SCAN_STEPS: usize = 10;

pub fn scan_environment(
    storage: &Storage,
    cancelled: &AtomicBool,
    scan_id: &str,
    on_progress: Arc<dyn Fn(ScanProgress) + Send + Sync>,
) -> Result<EnvironmentScan, AppError> {
    let scanned_at = Utc::now().to_rfc3339();
    let emit_progress = |phase, completed, manager_id| {
        on_progress(ScanProgress {
            scan_id: scan_id.into(),
            phase,
            completed,
            total: SCAN_STEPS,
            manager_id,
        })
    };
    emit_progress(ScanPhase::Managers, 0, None);
    let completed_managers = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let callback = {
        let completed_managers = completed_managers.clone();
        let scan_id = scan_id.to_string();
        let on_progress = on_progress.clone();
        Arc::new(move |manager_id| {
            let completed = completed_managers.fetch_add(1, Ordering::SeqCst) + 1;
            on_progress(ScanProgress {
                scan_id: scan_id.clone(),
                phase: ScanPhase::Managers,
                completed,
                total: SCAN_STEPS,
                manager_id: Some(manager_id),
            });
        })
    };
    let adapter_scans = adapters::scan_all(cancelled, callback)?;
    if cancelled.load(Ordering::SeqCst) {
        return Err(AppError::ScanCancelled);
    }
    let manager_count = adapter_scans.len();
    let mut managers = Vec::with_capacity(manager_count);
    let mut packages = Vec::new();
    let mut logs = Vec::new();
    let mut partial_failures = 0;

    for adapter_scan in adapter_scans {
        managers.push(adapter_scan.manager);
        packages.extend(adapter_scan.packages);
        logs.extend(adapter_scan.logs);
        partial_failures += adapter_scan.partial_failures;
    }
    packages.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));

    let roots = storage.list_scan_roots()?;
    let scan_roots = roots
        .iter()
        .map(|root| root.to_string_lossy().into_owned())
        .collect();
    emit_progress(ScanPhase::Projects, manager_count, None);
    let project_scan = projects::scan_projects(&roots, cancelled)?;
    if cancelled.load(Ordering::SeqCst) {
        return Err(AppError::ScanCancelled);
    }
    partial_failures += project_scan.failures;
    logs.extend(project_scan.logs);
    let path_observations = scan_paths();
    emit_progress(ScanPhase::Health, manager_count + 1, None);
    let health_issues = health::build_health_report(
        &managers,
        &packages,
        &project_scan.projects,
        &project_scan.workspaces,
        &path_observations,
    );
    logs.push(TaskLog {
        id: Uuid::new_v4().to_string(),
        category: LogCategory::Scan,
        status: if partial_failures == 0 {
            LogStatus::Success
        } else {
            LogStatus::Warning
        },
        message: if partial_failures == 0 {
            "环境扫描完成".into()
        } else {
            format!("环境扫描完成，{} 项读取失败", partial_failures)
        },
        manager_id: None,
        exit_code: None,
        output: None,
        timestamp: Utc::now().to_rfc3339(),
    });

    emit_progress(ScanPhase::Complete, SCAN_STEPS, None);
    Ok(EnvironmentScan {
        managers,
        packages,
        projects: project_scan.projects,
        dependency_insights: project_scan.dependency_insights,
        workspaces: project_scan.workspaces,
        scan_roots,
        health_issues,
        logs,
        path_observations,
        scanned_at,
        partial_failures,
    })
}

fn scan_paths() -> Vec<PathObservation> {
    ["node", "npm", "python3", "pip3"]
        .into_iter()
        .map(|command| {
            let paths = find_all_in_path(command);
            PathObservation {
                command: command.into(),
                active_path: paths
                    .first()
                    .map(|path| path.to_string_lossy().into_owned()),
                alternatives: paths
                    .iter()
                    .skip(1)
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect(),
                has_conflict: paths.len() > 1,
            }
        })
        .collect()
}

pub fn projects_for_roots(storage: &Storage) -> Result<ProjectAnalysis, AppError> {
    analyze_projects(&storage.list_scan_roots()?, &AtomicBool::new(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_scan_has_expected_commands() {
        let observations = scan_paths();
        assert_eq!(
            observations
                .iter()
                .map(|item| item.command.as_str())
                .collect::<Vec<_>>(),
            vec!["node", "npm", "python3", "pip3"]
        );
    }
}
