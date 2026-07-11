use crate::{
    error::AppError,
    models::{
        EnvironmentScan, ObservedActionOutcome, PackageAction, PackageActionStatus,
        PackageManagerId,
    },
    storage::Storage,
};

pub fn reconcile_pending_audits(
    storage: &Storage,
    current_snapshot_id: i64,
    current: &EnvironmentScan,
) -> Result<usize, AppError> {
    let mut reconciled = 0;
    for mut audit in storage.list_action_audit()? {
        if audit.status != PackageActionStatus::Running && !audit.rescan_required {
            continue;
        }
        let baseline = match audit.baseline_snapshot_id {
            Some(id) => storage.snapshot_by_id(id)?,
            None => None,
        };
        let (outcome, evidence) = match baseline.as_ref() {
            Some(baseline) => reconcile_observed_outcome(
                audit.manager_id,
                audit.action,
                &audit.targets,
                baseline,
                current,
            ),
            None => (
                ObservedActionOutcome::Ambiguous,
                vec!["基线快照不存在或已过期，只能确认已完成最新扫描。".into()],
            ),
        };
        if audit.status == PackageActionStatus::Running {
            audit.status = PackageActionStatus::Unknown;
            audit.error = Some("应用在操作完成前退出；已根据最新扫描核对实际环境。".into());
            audit.finished_at = chrono::Utc::now().to_rfc3339();
        }
        audit.result_snapshot_id = Some(current_snapshot_id);
        audit.observed_outcome = Some(outcome);
        audit.evidence = evidence;
        audit.reconciled_at = Some(chrono::Utc::now().to_rfc3339());
        audit.rescan_required = false;
        storage.save_action_audit(&audit)?;
        reconciled += 1;
    }
    Ok(reconciled)
}

pub fn reconcile_observed_outcome(
    manager_id: PackageManagerId,
    action: PackageAction,
    targets: &[String],
    baseline: &EnvironmentScan,
    current: &EnvironmentScan,
) -> (ObservedActionOutcome, Vec<String>) {
    match action {
        PackageAction::Install => reconcile_presence(manager_id, targets, baseline, current, true),
        PackageAction::Uninstall => {
            reconcile_presence(manager_id, targets, baseline, current, false)
        }
        PackageAction::Upgrade => reconcile_upgrade(manager_id, targets, baseline, current),
        PackageAction::Cleanup => reconcile_cache(manager_id, baseline, current),
    }
}

fn reconcile_presence(
    manager_id: PackageManagerId,
    targets: &[String],
    baseline: &EnvironmentScan,
    current: &EnvironmentScan,
    installing: bool,
) -> (ObservedActionOutcome, Vec<String>) {
    let mut evidence = Vec::new();
    let mut applied = true;
    let mut unchanged = true;
    for target in targets {
        let before = package_version(baseline, manager_id, target);
        let after = package_version(current, manager_id, target);
        evidence.push(format!(
            "{}：{} -> {}",
            target,
            before.unwrap_or("未安装"),
            after.unwrap_or("未安装")
        ));
        if installing {
            applied &= before.is_none() && after.is_some();
            unchanged &= before.is_none() && after.is_none();
        } else {
            applied &= before.is_some() && after.is_none();
            unchanged &= before.is_some() && after.is_some();
        }
    }
    (
        if applied {
            ObservedActionOutcome::Applied
        } else if unchanged {
            ObservedActionOutcome::NotApplied
        } else {
            ObservedActionOutcome::Ambiguous
        },
        evidence,
    )
}

fn reconcile_upgrade(
    manager_id: PackageManagerId,
    targets: &[String],
    baseline: &EnvironmentScan,
    current: &EnvironmentScan,
) -> (ObservedActionOutcome, Vec<String>) {
    let mut evidence = Vec::new();
    let mut changed = false;
    let mut all_present = true;
    for target in targets {
        let before = package_version(baseline, manager_id, target);
        let after = package_version(current, manager_id, target);
        evidence.push(format!(
            "{}：{} -> {}",
            target,
            before.unwrap_or("未安装"),
            after.unwrap_or("未安装")
        ));
        all_present &= before.is_some() && after.is_some();
        changed |= before.is_some() && after.is_some() && before != after;
    }
    (
        if all_present && changed {
            ObservedActionOutcome::Applied
        } else if all_present {
            ObservedActionOutcome::NotApplied
        } else {
            ObservedActionOutcome::Ambiguous
        },
        evidence,
    )
}

fn reconcile_cache(
    manager_id: PackageManagerId,
    baseline: &EnvironmentScan,
    current: &EnvironmentScan,
) -> (ObservedActionOutcome, Vec<String>) {
    let before = cache_size(baseline, manager_id);
    let after = cache_size(current, manager_id);
    let evidence = vec![format!(
        "缓存大小：{} -> {}",
        before.map(format_bytes).unwrap_or_else(|| "未知".into()),
        after.map(format_bytes).unwrap_or_else(|| "未知".into())
    )];
    (
        match (before, after) {
            (Some(before), Some(after)) if after < before => ObservedActionOutcome::Applied,
            (Some(before), Some(after)) if after == before => ObservedActionOutcome::NotApplied,
            _ => ObservedActionOutcome::Ambiguous,
        },
        evidence,
    )
}

fn package_version<'a>(
    scan: &'a EnvironmentScan,
    manager_id: PackageManagerId,
    name: &str,
) -> Option<&'a str> {
    scan.packages
        .iter()
        .find(|package| package.manager_id == manager_id && package.name == name)
        .map(|package| package.version.as_str())
}

fn cache_size(scan: &EnvironmentScan, manager_id: PackageManagerId) -> Option<u64> {
    scan.managers
        .iter()
        .find(|manager| manager.id == manager_id)
        .and_then(|manager| manager.cache_size_bytes)
}

fn format_bytes(value: u64) -> String {
    format!("{value} bytes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PackageActionAuditRecord;

    fn scan(packages: &[(&str, &str)], cache_size: Option<u64>) -> EnvironmentScan {
        serde_json::from_value(serde_json::json!({
            "managers": [{
                "id": "npm", "displayName": "npm", "status": "available",
                "executionTrust": "managed", "capabilities": [],
                "cacheSizeBytes": cache_size, "scannedAt": "2026-01-01T00:00:00Z"
            }],
            "packages": packages.iter().map(|(name, version)| serde_json::json!({
                "id": format!("npm:{name}"), "managerId": "npm", "name": name,
                "version": version, "scope": "global", "updateStatus": "unknown"
            })).collect::<Vec<_>>(),
            "projects": [], "scanRoots": [], "healthIssues": [], "logs": [],
            "pathObservations": [], "scannedAt": "2026-01-01T00:00:00Z", "partialFailures": 0
        }))
        .unwrap()
    }

    #[test]
    fn classifies_install_uninstall_upgrade_and_cache_observations() {
        let empty = scan(&[], Some(100));
        let installed = scan(&[("eslint", "9.0.0")], Some(80));
        assert_eq!(
            reconcile_observed_outcome(
                PackageManagerId::Npm,
                PackageAction::Install,
                &["eslint".into()],
                &empty,
                &installed
            )
            .0,
            ObservedActionOutcome::Applied
        );
        assert_eq!(
            reconcile_observed_outcome(
                PackageManagerId::Npm,
                PackageAction::Uninstall,
                &["eslint".into()],
                &installed,
                &empty
            )
            .0,
            ObservedActionOutcome::Applied
        );
        let upgraded = scan(&[("eslint", "9.1.0")], Some(80));
        assert_eq!(
            reconcile_observed_outcome(
                PackageManagerId::Npm,
                PackageAction::Upgrade,
                &["eslint".into()],
                &installed,
                &upgraded
            )
            .0,
            ObservedActionOutcome::Applied
        );
        assert_eq!(
            reconcile_observed_outcome(
                PackageManagerId::Npm,
                PackageAction::Cleanup,
                &[],
                &empty,
                &installed
            )
            .0,
            ObservedActionOutcome::Applied
        );
    }

    #[test]
    fn reconciles_interrupted_audit_and_clears_write_blocker() {
        let directory = tempfile::tempdir().unwrap();
        let storage = Storage::at(directory.path().join("test.sqlite3")).unwrap();
        let baseline = scan(&[], Some(100));
        storage.save_snapshot(&baseline).unwrap();
        let baseline_id = storage.list_snapshot_summaries().unwrap()[0].id;
        storage
            .save_action_audit(&PackageActionAuditRecord {
                action_id: "interrupted".into(),
                plan_id: "plan".into(),
                manager_id: PackageManagerId::Npm,
                action: PackageAction::Install,
                targets: vec!["eslint".into()],
                status: PackageActionStatus::Running,
                command_preview: "npm install --global --ignore-scripts eslint".into(),
                logs: vec![],
                error: None,
                started_at: "2026-01-01T00:00:00Z".into(),
                finished_at: "2026-01-01T00:00:00Z".into(),
                baseline_snapshot_id: Some(baseline_id),
                result_snapshot_id: None,
                observed_outcome: None,
                evidence: vec![],
                reconciled_at: None,
                rescan_required: true,
            })
            .unwrap();
        let current = scan(&[("eslint", "9.0.0")], Some(100));
        storage.save_snapshot(&current).unwrap();
        let current_id = storage.list_snapshot_summaries().unwrap()[0].id;
        assert_eq!(
            reconcile_pending_audits(&storage, current_id, &current).unwrap(),
            1
        );
        let audit = storage.action_audit("interrupted").unwrap().unwrap();
        assert_eq!(audit.status, PackageActionStatus::Unknown);
        assert_eq!(audit.observed_outcome, Some(ObservedActionOutcome::Applied));
        assert!(!audit.rescan_required);
        assert!(!storage.has_incomplete_action().unwrap());
    }
}
