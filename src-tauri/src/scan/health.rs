use std::collections::{HashMap, HashSet};

use crate::models::{
    HealthIssue, HealthSeverity, ManagedPackage, ManagerStatus, PackageManager, PackageManagerId,
    PackageScope, PathObservation, ProjectMetadata, ProjectWorkspace, RuntimeRequirementAssessment,
    RuntimeRequirementStatus, UpdateStatus,
};

const LARGE_CACHE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub fn build_health_report(
    managers: &[PackageManager],
    packages: &[ManagedPackage],
    projects: &[ProjectMetadata],
    workspaces: &[ProjectWorkspace],
    paths: &[PathObservation],
    runtime_assessments: &[RuntimeRequirementAssessment],
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
                command: None,
            });
        }
        if matches!(manager.status, ManagerStatus::Blocked) {
            issues.push(HealthIssue {
                id: format!("unverified-executable-{}", manager.id.as_str()),
                severity: HealthSeverity::Warning,
                code: "UNVERIFIED_EXECUTABLE".into(),
                title: format!("{} 的命令来源未经验证", manager.display_name),
                description: "为避免执行未知 PATH 中的程序，本次已仅展示发现结果。".into(),
                manager_id: Some(manager.id),
                path: manager.executable_path.clone(),
                command: None,
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
                command: None,
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
            command: None,
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
                command: None,
            });
        }
        for dependency in &project.dependencies {
            if dependency.resolution_checked && dependency.resolved_version.is_none() {
                issues.push(HealthIssue {
                    id: format!(
                        "dependency-unresolved-{}-{}",
                        project.path, dependency.normalized_name
                    ),
                    severity: HealthSeverity::Warning,
                    code: "DIRECT_DEPENDENCY_NOT_RESOLVED".into(),
                    title: format!("{} 未在锁文件中解析 {}", project.name, dependency.name),
                    description: "已找到对应锁文件，但没有匹配的直接依赖已解析版本。".into(),
                    manager_id: None,
                    path: Some(project.path.clone()),
                    command: None,
                });
            }
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
                    command: None,
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
                    command: None,
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
                command: None,
            });
        }
    }

    let mut resolved_versions =
        std::collections::BTreeMap::<(String, String), std::collections::BTreeSet<String>>::new();
    for project in projects {
        for dependency in &project.dependencies {
            if let Some(version) = &dependency.resolved_version {
                resolved_versions
                    .entry((
                        dependency.ecosystem.clone(),
                        dependency.normalized_name.clone(),
                    ))
                    .or_default()
                    .insert(version.clone());
            }
        }
    }
    for ((ecosystem, name), versions) in resolved_versions
        .into_iter()
        .filter(|(_, values)| values.len() > 1)
    {
        issues.push(HealthIssue {
            id: format!("resolved-version-divergence-{ecosystem}-{name}"),
            severity: HealthSeverity::Warning,
            code: "RESOLVED_VERSION_DIVERGENCE".into(),
            title: format!("{name} 的已解析版本存在分歧"),
            description: format!(
                "{ecosystem} 锁文件解析出多个版本：{}。",
                versions.into_iter().collect::<Vec<_>>().join("、")
            ),
            manager_id: None,
            path: None,
            command: None,
        });
    }

    for path in paths.iter().filter(|path| path.has_conflict) {
        issues.push(HealthIssue {
            id: format!("command-path-conflict-{}", path.command),
            severity: HealthSeverity::Warning,
            code: "COMMAND_PATH_CONFLICT".into(),
            title: format!("发现多个 {} 路径", path.command),
            description: format!(
                "当前优先使用 {}，另有 {} 个候选路径。",
                path.active_path.as_deref().unwrap_or("未知路径"),
                path.alternatives.len()
            ),
            manager_id: None,
            path: path.active_path.clone(),
            command: Some(path.command.clone()),
        });
    }
    for (runtime, manager) in [("node", "npm"), ("python", "pip")] {
        let runtime_path = paths
            .iter()
            .find(|path| path.command == runtime)
            .and_then(|path| path.active_path.as_deref());
        let manager_path = paths
            .iter()
            .find(|path| path.command == manager)
            .and_then(|path| path.active_path.as_deref());
        if runtime_path.is_some_and(|runtime_path| {
            manager_path.is_some_and(|manager_path| {
                std::path::Path::new(runtime_path).parent()
                    != std::path::Path::new(manager_path).parent()
            })
        }) {
            issues.push(HealthIssue {
                id: format!("runtime-manager-mismatch-{runtime}-{manager}"),
                severity: HealthSeverity::Warning,
                code: "RUNTIME_MANAGER_MISMATCH".into(),
                title: format!("{runtime} 与 {manager} 的生效路径不一致"),
                description: format!(
                    "{runtime} 优先使用 {}，{manager} 优先使用 {}。请确认 PATH 顺序符合预期。",
                    runtime_path.unwrap_or_default(),
                    manager_path.unwrap_or_default()
                ),
                manager_id: None,
                path: runtime_path.map(str::to_owned),
                command: Some(runtime.into()),
            });
        }
    }
    let node_managers = [
        PackageManagerId::Npm,
        PackageManagerId::Pnpm,
        PackageManagerId::Yarn,
        PackageManagerId::Bun,
    ];
    let mut global_tools = HashMap::<String, HashSet<PackageManagerId>>::new();
    for package in packages.iter().filter(|package| {
        package.scope == PackageScope::Global && node_managers.contains(&package.manager_id)
    }) {
        global_tools
            .entry(package.name.to_lowercase())
            .or_default()
            .insert(package.manager_id);
    }
    for (name, managers) in global_tools
        .into_iter()
        .filter(|(_, managers)| managers.len() > 1)
    {
        let mut sources = managers
            .iter()
            .map(|manager| manager.as_str())
            .collect::<Vec<_>>();
        sources.sort_unstable();
        issues.push(HealthIssue {
            id: format!("global-tool-duplicate-{name}"),
            severity: HealthSeverity::Info,
            code: "GLOBAL_TOOL_DUPLICATE".into(),
            title: format!("全局工具 {name} 在多个管理器中重复安装"),
            description: format!(
                "检测到来源：{}。当前版本仅展示信息，不会卸载或修改环境。",
                sources.join("、")
            ),
            manager_id: None,
            path: None,
            command: None,
        });
    }
    for assessment in runtime_assessments {
        let (code, title) = match assessment.status {
            RuntimeRequirementStatus::Missing => (
                "RUNTIME_NOT_INSTALLED",
                format!(
                    "{} 缺少 {} 运行时",
                    assessment.project_name, assessment.runtime
                ),
            ),
            RuntimeRequirementStatus::Mismatch => (
                "ACTIVE_RUNTIME_MISMATCH",
                format!(
                    "{} 的 {} 当前版本不匹配",
                    assessment.project_name, assessment.runtime
                ),
            ),
            RuntimeRequirementStatus::Available | RuntimeRequirementStatus::Unknown => continue,
        };
        issues.push(HealthIssue {
            id: format!(
                "runtime-requirement-{}-{}",
                assessment.project_path,
                assessment.runtime.to_lowercase()
            ),
            severity: HealthSeverity::Warning,
            code: code.into(),
            title,
            description: assessment.message.clone(),
            manager_id: None,
            path: Some(assessment.project_path.clone()),
            command: None,
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
        ManagerStatus, PackageManagerId, PackageScope, PathObservation, ProjectDependency,
        ProjectMetadata, ProjectWorkspace, RuntimeRequirementAssessment, RuntimeRequirementStatus,
    };

    #[test]
    fn reports_large_cache() {
        let manager = PackageManager {
            id: PackageManagerId::Pnpm,
            display_name: "pnpm".into(),
            version: Some("1".into()),
            executable_path: Some("/tmp/pnpm".into()),
            status: ManagerStatus::Available,
            execution_trust: crate::models::ExecutionTrust::NotApplicable,
            capabilities: vec![],
            error: None,
            cache_size_bytes: Some(LARGE_CACHE_BYTES),
            cache_scan_status: crate::models::CacheScanStatus::Complete,
            scanned_at: String::new(),
        };
        let issues = build_health_report(&[manager], &[], &[], &[], &[], &[]);
        assert_eq!(issues[0].code, "LARGE_CACHE");
    }

    #[test]
    fn reports_blocked_unverified_executable() {
        let manager = PackageManager {
            id: PackageManagerId::Npm,
            display_name: "npm".into(),
            version: None,
            executable_path: Some("/tmp/unverified/npm".into()),
            status: ManagerStatus::Blocked,
            execution_trust: crate::models::ExecutionTrust::Unverified,
            capabilities: vec![],
            error: None,
            cache_size_bytes: None,
            cache_scan_status: crate::models::CacheScanStatus::NotApplicable,
            scanned_at: String::new(),
        };
        let issues = build_health_report(&[manager], &[], &[], &[], &[], &[]);
        assert_eq!(issues[0].code, "UNVERIFIED_EXECUTABLE");
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
                    resolved_version: None,
                    resolution_source: None,
                    resolution_checked: false,
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
                    resolved_version: None,
                    resolution_source: None,
                    resolution_checked: false,
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
        let issues = build_health_report(&[], &[], &projects, &workspaces, &[], &[]);
        assert!(issues
            .iter()
            .any(|issue| issue.code == "WORKSPACE_DEPENDENCY_VERSION_DIVERGENCE"));
        assert!(issues
            .iter()
            .any(|issue| issue.code == "DEPENDENCY_VERSION_UNSPECIFIED"));
    }

    #[test]
    fn reports_command_conflicts_runtime_mismatches_and_duplicate_global_tools() {
        let paths = vec![
            PathObservation {
                command: "node".into(),
                active_path: Some("/Users/example/.nvm/bin/node".into()),
                alternatives: vec!["/opt/homebrew/bin/node".into()],
                has_conflict: true,
                candidates: vec![],
            },
            PathObservation {
                command: "npm".into(),
                active_path: Some("/opt/homebrew/bin/npm".into()),
                alternatives: vec![],
                has_conflict: false,
                candidates: vec![],
            },
        ];
        let packages = vec![
            ManagedPackage {
                id: "npm:typescript".into(),
                manager_id: PackageManagerId::Npm,
                name: "typescript".into(),
                version: "5.9.3".into(),
                latest_version: None,
                scope: PackageScope::Global,
                update_status: UpdateStatus::Unknown,
            },
            ManagedPackage {
                id: "pnpm:typescript".into(),
                manager_id: PackageManagerId::Pnpm,
                name: "typescript".into(),
                version: "5.9.3".into(),
                latest_version: None,
                scope: PackageScope::Global,
                update_status: UpdateStatus::Unknown,
            },
        ];
        let issues = build_health_report(&[], &packages, &[], &[], &paths, &[]);
        assert!(issues
            .iter()
            .any(|issue| issue.code == "COMMAND_PATH_CONFLICT"
                && issue.command.as_deref() == Some("node")));
        assert!(issues
            .iter()
            .any(|issue| issue.code == "RUNTIME_MANAGER_MISMATCH"));
        assert!(issues
            .iter()
            .any(|issue| issue.code == "GLOBAL_TOOL_DUPLICATE"));
    }

    #[test]
    fn reports_missing_and_mismatched_project_runtimes() {
        let assessments = vec![
            RuntimeRequirementAssessment {
                project_name: "api".into(),
                project_path: "/tmp/api".into(),
                runtime: "Python".into(),
                requirement: "3.12.0".into(),
                status: RuntimeRequirementStatus::Missing,
                active_version: None,
                installed_versions: vec![],
                message: "未发现可用的 Python 运行时".into(),
            },
            RuntimeRequirementAssessment {
                project_name: "web".into(),
                project_path: "/tmp/web".into(),
                runtime: "Node.js".into(),
                requirement: "22.18.0".into(),
                status: RuntimeRequirementStatus::Mismatch,
                active_version: Some("22.17.0".into()),
                installed_versions: vec!["22.17.0".into()],
                message: "当前激活版本不一致".into(),
            },
        ];
        let issues = build_health_report(&[], &[], &[], &[], &[], &assessments);
        assert!(issues
            .iter()
            .any(|issue| issue.code == "RUNTIME_NOT_INSTALLED"));
        assert!(issues
            .iter()
            .any(|issue| issue.code == "ACTIVE_RUNTIME_MISMATCH"));
    }
}
