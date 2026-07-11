use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    error::AppError,
    models::{
        EnvironmentScan, SnapshotChange, SnapshotChangeEntity, SnapshotChangeKind,
        SnapshotComparison, SnapshotSummary,
    },
};

pub fn compare_snapshots(
    baseline_id: i64,
    baseline: &EnvironmentScan,
    current_id: i64,
    current: &EnvironmentScan,
) -> Result<SnapshotComparison, AppError> {
    let mut changes = Vec::new();
    compare_managers(baseline, current, &mut changes)?;
    compare_packages(baseline, current, &mut changes)?;
    compare_projects(baseline, current, &mut changes)?;
    compare_health_issues(baseline, current, &mut changes)?;
    changes.sort_by(|left, right| {
        (left.entity, left.kind, &left.title).cmp(&(right.entity, right.kind, &right.title))
    });

    let added_count = changes
        .iter()
        .filter(|change| change.kind == SnapshotChangeKind::Added)
        .count();
    let removed_count = changes
        .iter()
        .filter(|change| change.kind == SnapshotChangeKind::Removed)
        .count();
    let changed_count = changes
        .iter()
        .filter(|change| change.kind == SnapshotChangeKind::Changed)
        .count();
    Ok(SnapshotComparison {
        baseline: SnapshotSummary::from_scan(baseline_id, baseline),
        current: SnapshotSummary::from_scan(current_id, current),
        changes,
        added_count,
        removed_count,
        changed_count,
    })
}

fn compare_managers(
    baseline: &EnvironmentScan,
    current: &EnvironmentScan,
    changes: &mut Vec<SnapshotChange>,
) -> Result<(), AppError> {
    let before = baseline
        .managers
        .iter()
        .map(|manager| (manager.id.as_str(), manager))
        .collect::<BTreeMap<_, _>>();
    let after = current
        .managers
        .iter()
        .map(|manager| (manager.id.as_str(), manager))
        .collect::<BTreeMap<_, _>>();
    for (id, manager) in &after {
        match before.get(id) {
            None => changes.push(change(
                SnapshotChangeKind::Added,
                SnapshotChangeEntity::Manager,
                id,
                format!("新增包管理器：{}", manager.display_name),
                "本次扫描发现该包管理器。",
            )),
            Some(previous) if manager_differs(previous, manager)? => changes.push(change(
                SnapshotChangeKind::Changed,
                SnapshotChangeEntity::Manager,
                id,
                format!("包管理器已变化：{}", manager.display_name),
                "版本、状态、可执行路径或缓存信息已变化。",
            )),
            _ => {}
        }
    }
    for (id, manager) in &before {
        if !after.contains_key(id) {
            changes.push(change(
                SnapshotChangeKind::Removed,
                SnapshotChangeEntity::Manager,
                id,
                format!("未再发现包管理器：{}", manager.display_name),
                "该管理器未出现在本次扫描结果中。",
            ));
        }
    }
    Ok(())
}

fn compare_packages(
    baseline: &EnvironmentScan,
    current: &EnvironmentScan,
    changes: &mut Vec<SnapshotChange>,
) -> Result<(), AppError> {
    let before = baseline
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package))
        .collect::<BTreeMap<_, _>>();
    let after = current
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package))
        .collect::<BTreeMap<_, _>>();
    for (id, package) in &after {
        match before.get(id) {
            None => changes.push(change(
                SnapshotChangeKind::Added,
                SnapshotChangeEntity::Package,
                id,
                format!("新增软件包：{}", package.name),
                &format!("{} · {}", package.manager_id.as_str(), package.version),
            )),
            Some(previous) if differs(*previous, *package)? => changes.push(change(
                SnapshotChangeKind::Changed,
                SnapshotChangeEntity::Package,
                id,
                format!("软件包已变化：{}", package.name),
                &format!(
                    "{}：{} → {}",
                    package.manager_id.as_str(),
                    previous.version,
                    package.version
                ),
            )),
            _ => {}
        }
    }
    for (id, package) in &before {
        if !after.contains_key(id) {
            changes.push(change(
                SnapshotChangeKind::Removed,
                SnapshotChangeEntity::Package,
                id,
                format!("已移除软件包：{}", package.name),
                &format!("{} · {}", package.manager_id.as_str(), package.version),
            ));
        }
    }
    Ok(())
}

fn compare_projects(
    baseline: &EnvironmentScan,
    current: &EnvironmentScan,
    changes: &mut Vec<SnapshotChange>,
) -> Result<(), AppError> {
    let before = baseline
        .projects
        .iter()
        .map(|project| (project.path.as_str(), project))
        .collect::<BTreeMap<_, _>>();
    let after = current
        .projects
        .iter()
        .map(|project| (project.path.as_str(), project))
        .collect::<BTreeMap<_, _>>();
    for (path, project) in &after {
        match before.get(path) {
            None => changes.push(change(
                SnapshotChangeKind::Added,
                SnapshotChangeEntity::Project,
                path,
                format!("新增项目：{}", project.name),
                path,
            )),
            Some(previous) if differs(*previous, *project)? => changes.push(change(
                SnapshotChangeKind::Changed,
                SnapshotChangeEntity::Project,
                path,
                format!("项目元数据已变化：{}", project.name),
                "生态、锁文件、运行时或直接依赖声明已变化。",
            )),
            _ => {}
        }
    }
    for (path, project) in &before {
        if !after.contains_key(path) {
            changes.push(change(
                SnapshotChangeKind::Removed,
                SnapshotChangeEntity::Project,
                path,
                format!("未再识别项目：{}", project.name),
                path,
            ));
        }
    }
    Ok(())
}

fn compare_health_issues(
    baseline: &EnvironmentScan,
    current: &EnvironmentScan,
    changes: &mut Vec<SnapshotChange>,
) -> Result<(), AppError> {
    let before = baseline
        .health_issues
        .iter()
        .map(|issue| (issue.id.as_str(), issue))
        .collect::<BTreeMap<_, _>>();
    let after = current
        .health_issues
        .iter()
        .map(|issue| (issue.id.as_str(), issue))
        .collect::<BTreeMap<_, _>>();
    for (id, issue) in &after {
        match before.get(id) {
            None => changes.push(change(
                SnapshotChangeKind::Added,
                SnapshotChangeEntity::Health,
                id,
                format!("新增健康提示：{}", issue.title),
                &issue.description,
            )),
            Some(previous) if differs(*previous, *issue)? => changes.push(change(
                SnapshotChangeKind::Changed,
                SnapshotChangeEntity::Health,
                id,
                format!("健康提示已变化：{}", issue.title),
                &issue.description,
            )),
            _ => {}
        }
    }
    for (id, issue) in &before {
        if !after.contains_key(id) {
            changes.push(change(
                SnapshotChangeKind::Removed,
                SnapshotChangeEntity::Health,
                id,
                format!("健康提示已消失：{}", issue.title),
                "该提示未出现在本次扫描结果中。",
            ));
        }
    }
    Ok(())
}

fn differs<T: Serialize>(left: &T, right: &T) -> Result<bool, AppError> {
    Ok(serde_json::to_value(left)? != serde_json::to_value(right)?)
}

fn manager_differs(
    left: &crate::models::PackageManager,
    right: &crate::models::PackageManager,
) -> Result<bool, AppError> {
    let mut left = serde_json::to_value(left)?;
    let mut right = serde_json::to_value(right)?;
    if let Some(value) = left.as_object_mut() {
        value.remove("scannedAt");
    }
    if let Some(value) = right.as_object_mut() {
        value.remove("scannedAt");
    }
    Ok(left != right)
}

fn change(
    kind: SnapshotChangeKind,
    entity: SnapshotChangeEntity,
    key: &str,
    title: String,
    description: &str,
) -> SnapshotChange {
    SnapshotChange {
        kind,
        entity,
        key: key.into(),
        title,
        description: description.into(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn scan(value: serde_json::Value) -> EnvironmentScan {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn detects_additions_removals_and_changes_by_stable_keys() {
        let baseline = scan(json!({
            "managers":[{"id":"npm","displayName":"npm","version":"10","status":"available","capabilities":[],"scannedAt":"old"}],
            "packages":[{"id":"npm:typescript","managerId":"npm","name":"typescript","version":"5.8","scope":"global","updateStatus":"upToDate"}],
            "projects":[{"name":"app","path":"/Users/demo/Code/app","ecosystems":["JavaScript"],"lockFiles":["package-lock.json"],"runtimeRequirements":[],"dependencies":[],"warnings":[]}],
            "scanRoots":[],"healthIssues":[{"id":"updates","severity":"warning","code":"UPDATES_AVAILABLE","title":"可更新","description":"old"}],"logs":[],"pathObservations":[],"scannedAt":"old","partialFailures":0
        }));
        let current = scan(json!({
            "managers":[{"id":"npm","displayName":"npm","version":"11","status":"available","capabilities":[],"scannedAt":"new"},{"id":"bun","displayName":"Bun","version":"1","status":"available","capabilities":[],"scannedAt":"new"}],
            "packages":[{"id":"npm:typescript","managerId":"npm","name":"typescript","version":"5.9","scope":"global","updateStatus":"available"},{"id":"bun:hono","managerId":"bun","name":"hono","version":"4","scope":"global","updateStatus":"unknown"}],
            "projects":[{"name":"app","path":"/Users/demo/Code/app","ecosystems":["JavaScript"],"lockFiles":["pnpm-lock.yaml"],"runtimeRequirements":[],"dependencies":[],"warnings":[]}],
            "scanRoots":[],"healthIssues":[],"logs":[],"pathObservations":[],"scannedAt":"new","partialFailures":0
        }));

        let comparison = compare_snapshots(1, &baseline, 2, &current).unwrap();
        assert_eq!(comparison.added_count, 2);
        assert_eq!(comparison.removed_count, 1);
        assert_eq!(comparison.changed_count, 3);
        assert!(comparison
            .changes
            .iter()
            .any(|change| change.key == "npm:typescript"
                && change.kind == SnapshotChangeKind::Changed));
    }

    #[test]
    fn ignores_manager_scan_timestamp_when_comparing_unchanged_environment() {
        let baseline = scan(json!({
            "managers":[{"id":"npm","displayName":"npm","version":"11","status":"available","capabilities":[],"scannedAt":"old"}],
            "packages":[],"projects":[],"scanRoots":[],"healthIssues":[],"logs":[],"pathObservations":[],"scannedAt":"old","partialFailures":0
        }));
        let mut current = baseline.clone();
        current.scanned_at = "new".into();
        current.managers[0].scanned_at = "new".into();

        assert!(compare_snapshots(1, &baseline, 2, &current)
            .unwrap()
            .changes
            .is_empty());
    }
}
