use crate::models::{
    HealthIssue, HealthSeverity, ManagedPackage, ManagerStatus, PackageManager, PathObservation,
    ProjectMetadata, ProjectWorkspace, UpdateStatus,
};

const LARGE_CACHE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub fn build_health_report(
    managers: &[PackageManager],
    packages: &[ManagedPackage],
    projects: &[ProjectMetadata],
    workspaces: &[ProjectWorkspace],
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
        for dependency in &project.dependencies {
            if dependency.version_requirement == "未声明版本" {
                issues.push(HealthIssue {
                    id: format!(
                        "dependency-unversioned-{}-{}",
                        project.path, dependency.normalized_name
                    ),
                    severity: HealthSeverity::Warning,
                    code: "DEPENDENCY_VERSION_UNSPECIFIED".into(),
                    title: format!("{} 未声明 {} 的版本", project.name, dependency.name),
                    description: "无法可靠比较该依赖在项目间的版本范围。".into(),
                    manager_id: None,
                    path: Some(project.path.clone()),
                });
            }
            if is_local_dependency(&dependency.version_requirement) {
                issues.push(HealthIssue {
                    id: format!(
                        "dependency-local-{}-{}",
                        project.path, dependency.normalized_name
                    ),
                    severity: HealthSeverity::Info,
                    code: "LOCAL_DEPENDENCY_REFERENCE".into(),
                    title: format!("{} 使用本地依赖 {}", project.name, dependency.name),
                    description: format!(
                        "声明为 {}，仅作只读提示。",
                        dependency.version_requirement
                    ),
                    manager_id: None,
                    path: Some(project.path.clone()),
                });
            }
        }
    }

    for workspace in workspaces {
        let members = projects
            .iter()
            .filter(|project| workspace.member_paths.contains(&project.path));
        let mut versions = std::collections::BTreeMap::<
            (String, String),
            std::collections::BTreeSet<String>,
        >::new();
        for project in members {
            for dependency in &project.dependencies {
                versions
                    .entry((
                        dependency.ecosystem.clone(),
                        dependency.normalized_name.clone(),
                    ))
                    .or_default()
                    .insert(dependency.version_requirement.clone());
            }
        }
        for ((ecosystem, name), requirements) in
            versions.into_iter().filter(|(_, values)| values.len() > 1)
        {
            issues.push(HealthIssue {
                id: format!(
                    "workspace-dependency-divergence-{}-{}",
                    workspace.path, name
                ),
                severity: HealthSeverity::Warning,
                code: "WORKSPACE_DEPENDENCY_VERSION_DIVERGENCE".into(),
                title: format!("{} 中 {} 版本范围不一致", workspace.name, name),
                description: format!(
                    "{ecosystem} 直接依赖声明为 {}。",
                    requirements.into_iter().collect::<Vec<_>>().join("、")
                ),
                manager_id: None,
                path: Some(workspace.path.clone()),
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

fn is_local_dependency(requirement: &str) -> bool {
    requirement.starts_with("workspace:")
        || requirement.starts_with("file:")
        || requirement.starts_with("link:")
        || requirement.starts_with("path:")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ManagerStatus, PackageManagerId, ProjectDependency, ProjectMetadata, ProjectWorkspace,
    };

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
        let issues = build_health_report(&[manager], &[], &[], &[], &[]);
        assert_eq!(issues[0].code, "LARGE_CACHE");
    }

    #[test]
    fn reports_workspace_divergence_and_unversioned_dependencies() {
        let projects = vec![
            ProjectMetadata {
                name: "web".into(),
                path: "/repo/apps/web".into(),
                ecosystems: vec!["JavaScript".into()],
                lock_files: vec![],
                runtime_requirements: vec![],
                package_manager: None,
                dependencies: vec![ProjectDependency {
                    ecosystem: "JavaScript".into(),
                    name: "shared".into(),
                    normalized_name: "shared".into(),
                    version_requirement: "^1".into(),
                    scopes: vec!["运行".into()],
                }],
                workspace: None,
                warnings: vec![],
            },
            ProjectMetadata {
                name: "docs".into(),
                path: "/repo/apps/docs".into(),
                ecosystems: vec!["JavaScript".into()],
                lock_files: vec![],
                runtime_requirements: vec![],
                package_manager: None,
                dependencies: vec![ProjectDependency {
                    ecosystem: "JavaScript".into(),
                    name: "shared".into(),
                    normalized_name: "shared".into(),
                    version_requirement: "未声明版本".into(),
                    scopes: vec!["运行".into()],
                }],
                workspace: None,
                warnings: vec![],
            },
        ];
        let workspaces = vec![ProjectWorkspace {
            name: "repo".into(),
            path: "/repo".into(),
            ecosystem: "JavaScript".into(),
            member_paths: projects
                .iter()
                .map(|project| project.path.clone())
                .collect(),
        }];
        let issues = build_health_report(&[], &[], &projects, &workspaces, &[]);
        assert!(issues
            .iter()
            .any(|issue| issue.code == "WORKSPACE_DEPENDENCY_VERSION_DIVERGENCE"));
        assert!(issues
            .iter()
            .any(|issue| issue.code == "DEPENDENCY_VERSION_UNSPECIFIED"));
    }
}
