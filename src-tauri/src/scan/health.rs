use crate::models::{
    HealthIssue, HealthSeverity, ManagedPackage, ManagerStatus, PackageManager, PathObservation,
    ProjectMetadata, UpdateStatus,
};

const LARGE_CACHE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub fn build_health_report(
    managers: &[PackageManager],
    packages: &[ManagedPackage],
    projects: &[ProjectMetadata],
    paths: &[PathObservation],
) -> Vec<HealthIssue> {
    let mut issues = Vec::new();
    for manager in managers {
        if matches!(manager.status, ManagerStatus::Error) {
            issues.push(HealthIssue {
                id: format!("manager-error-{}", manager.id.as_str()),
                severity: HealthSeverity::Error,
                code: "MANAGER_COMMAND_FAILED".into(),
                title: format!("{} 无法正常运行", manager.display_name),
                description: manager
                    .error
                    .as_ref()
                    .map(|error| error.message.clone())
                    .unwrap_or_else(|| "版本检查失败".into()),
                manager_id: Some(manager.id),
                path: manager.executable_path.clone(),
            });
        }
        if manager
            .cache_size_bytes
            .is_some_and(|size| size >= LARGE_CACHE_BYTES)
        {
            issues.push(HealthIssue {
                id: format!("large-cache-{}", manager.id.as_str()),
                severity: HealthSeverity::Warning,
                code: "LARGE_CACHE".into(),
                title: format!("{} 缓存占用较大", manager.display_name),
                description: "缓存已超过 2 GB。当前版本只展示空间信息，不会自动清理。".into(),
                manager_id: Some(manager.id),
                path: None,
            });
        }
    }

    let update_count = packages
        .iter()
        .filter(|package| package.update_status == UpdateStatus::Available)
        .count();
    if update_count > 0 {
        issues.push(HealthIssue {
            id: "updates-available".into(),
            severity: HealthSeverity::Warning,
            code: "UPDATES_AVAILABLE".into(),
            title: format!("{} 个软件包可更新", update_count),
            description: "当前版本仅展示更新状态，不会执行升级操作。".into(),
            manager_id: None,
            path: None,
        });
    }

    for project in projects {
        for (index, warning) in project.warnings.iter().enumerate() {
            issues.push(HealthIssue {
                id: format!("project-warning-{}-{}", project.path, index),
                severity: HealthSeverity::Warning,
                code: "PROJECT_CONFIGURATION_MISMATCH".into(),
                title: format!("{} 的包管理配置不一致", project.name),
                description: warning.clone(),
                manager_id: None,
                path: Some(project.path.clone()),
            });
        }
    }

    for path in paths
        .iter()
        .filter(|path| path.has_conflict && matches!(path.command.as_str(), "node" | "python3"))
    {
        issues.push(HealthIssue {
            id: format!("path-conflict-{}", path.command),
            severity: HealthSeverity::Info,
            code: "PATH_CONFLICT".into(),
            title: format!("发现多个 {} 路径", path.command),
            description: format!(
                "当前优先使用 {}，另有 {} 个候选路径。",
                path.active_path.as_deref().unwrap_or("未知路径"),
                path.alternatives.len()
            ),
            manager_id: None,
            path: path.active_path.clone(),
        });
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ManagerStatus, PackageManagerId};

    #[test]
    fn reports_large_cache() {
        let manager = PackageManager {
            id: PackageManagerId::Pnpm,
            display_name: "pnpm".into(),
            version: Some("1".into()),
            executable_path: Some("/tmp/pnpm".into()),
            status: ManagerStatus::Available,
            capabilities: vec![],
            error: None,
            cache_size_bytes: Some(LARGE_CACHE_BYTES),
            scanned_at: String::new(),
        };
        let issues = build_health_report(&[manager], &[], &[], &[]);
        assert_eq!(issues[0].code, "LARGE_CACHE");
    }
}
