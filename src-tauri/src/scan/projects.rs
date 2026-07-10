use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use chrono::Utc;
use serde_json::Value as JsonValue;
use toml::Value as TomlValue;
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use crate::{
    error::AppError,
    models::{
        DependencyInsight, DependencyProjectUsage, LogCategory, LogStatus, ProjectAnalysis,
        ProjectDependency, ProjectMetadata, RuntimeRequirement, TaskLog,
    },
};

const MARKERS: [&str; 11] = [
    "package.json",
    "pyproject.toml",
    "requirements.txt",
    "pnpm-lock.yaml",
    "package-lock.json",
    "uv.lock",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
    "Cargo.toml",
    "Cargo.lock",
];
const MAX_DEPTH: usize = 6;

#[derive(Debug)]
pub struct ProjectScan {
    pub projects: Vec<ProjectMetadata>,
    pub dependency_insights: Vec<DependencyInsight>,
    pub logs: Vec<TaskLog>,
    pub failures: usize,
}

pub fn scan_projects(roots: &[PathBuf], cancelled: &AtomicBool) -> Result<ProjectScan, AppError> {
    let mut directories: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
    let mut logs = Vec::new();
    let mut failures = 0;

    for root in roots {
        if !root.is_dir() {
            failures += 1;
            logs.push(project_log(
                LogStatus::Error,
                &format!("扫描目录不存在：{}", root.display()),
            ));
            continue;
        }
        for entry in WalkDir::new(root)
            .max_depth(MAX_DEPTH)
            .follow_links(false)
            .into_iter()
            .filter_entry(should_visit)
        {
            if cancelled.load(Ordering::SeqCst) {
                return Err(AppError::ScanCancelled);
            }
            match entry {
                Ok(entry) if entry.file_type().is_file() => {
                    let file_name = entry.file_name().to_string_lossy();
                    if MARKERS.contains(&file_name.as_ref()) {
                        if let Some(parent) = entry.path().parent() {
                            directories
                                .entry(parent.to_path_buf())
                                .or_default()
                                .insert(file_name.into_owned());
                        }
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    failures += 1;
                    logs.push(project_log(
                        LogStatus::Warning,
                        &format!("部分目录无法读取：{}", error),
                    ));
                }
            }
        }
    }

    let projects = directories
        .into_iter()
        .map(|(directory, markers)| parse_project(&directory, &markers))
        .collect::<Vec<_>>();
    if !roots.is_empty() {
        logs.push(project_log(
            LogStatus::Success,
            &format!("项目扫描完成，共识别 {} 个项目", projects.len()),
        ));
    }
    let dependency_insights = dependency_insights(&projects);
    Ok(ProjectScan {
        projects,
        dependency_insights,
        logs,
        failures,
    })
}

pub fn analyze_projects(
    roots: &[PathBuf],
    cancelled: &AtomicBool,
) -> Result<ProjectAnalysis, AppError> {
    let scan = scan_projects(roots, cancelled)?;
    Ok(ProjectAnalysis {
        projects: scan.projects,
        dependency_insights: scan.dependency_insights,
    })
}

fn should_visit(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    if !entry.file_type().is_dir() {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    !name.starts_with('.')
        && !matches!(
            name.as_ref(),
            "node_modules"
                | "target"
                | "dist"
                | "build"
                | ".next"
                | "coverage"
                | "vendor"
                | "venv"
                | ".venv"
                | "__pycache__"
        )
}

fn parse_project(directory: &Path, markers: &BTreeSet<String>) -> ProjectMetadata {
    let mut name = directory
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project")
        .to_string();
    let mut ecosystems = Vec::new();
    let mut runtime_requirements = Vec::new();
    let mut package_manager = None;
    let mut warnings = Vec::new();
    let mut dependencies = Vec::new();

    if markers.contains("package.json") {
        ecosystems.push("JavaScript".into());
        if let Ok(value) = fs::read_to_string(directory.join("package.json")).and_then(|value| {
            serde_json::from_str::<JsonValue>(&value).map_err(std::io::Error::other)
        }) {
            if let Some(project_name) = value.get("name").and_then(JsonValue::as_str) {
                name = project_name.into();
            }
            package_manager = value
                .get("packageManager")
                .and_then(JsonValue::as_str)
                .map(str::to_string);
            if let Some(node) = value.pointer("/engines/node").and_then(JsonValue::as_str) {
                runtime_requirements.push(RuntimeRequirement {
                    runtime: "Node.js".into(),
                    requirement: node.into(),
                });
            }
            collect_javascript_dependencies(&value, &mut dependencies, &mut warnings);
        }
        if package_manager.is_none() {
            package_manager = package_manager_from_locks(markers, &mut warnings);
        }
    }

    if markers.contains("Cargo.toml") {
        ecosystems.push("Rust".into());
        package_manager = Some("cargo".into());
        if let Ok(source) = fs::read_to_string(directory.join("Cargo.toml")) {
            if let Ok(value) = source.parse::<TomlValue>() {
                if !markers.contains("package.json") {
                    if let Some(project_name) = value
                        .get("package")
                        .and_then(|value| value.get("name"))
                        .and_then(TomlValue::as_str)
                    {
                        name = project_name.into();
                    }
                }
                if let Some(requirement) = value
                    .get("package")
                    .and_then(|value| value.get("rust-version"))
                    .and_then(TomlValue::as_str)
                {
                    runtime_requirements.push(RuntimeRequirement {
                        runtime: "Rust".into(),
                        requirement: requirement.into(),
                    });
                }
                collect_rust_dependencies(&value, &mut dependencies, &mut warnings);
            }
        }
    }

    if markers.contains("pyproject.toml") || markers.contains("requirements.txt") {
        ecosystems.push("Python".into());
        if let Ok(source) = fs::read_to_string(directory.join("pyproject.toml")) {
            if let Ok(value) = source.parse::<TomlValue>() {
                if !markers.contains("package.json") {
                    if let Some(project_name) = value
                        .get("project")
                        .and_then(|value| value.get("name"))
                        .and_then(TomlValue::as_str)
                    {
                        name = project_name.into();
                    }
                }
                if let Some(requirement) = value
                    .get("project")
                    .and_then(|value| value.get("requires-python"))
                    .and_then(TomlValue::as_str)
                {
                    runtime_requirements.push(RuntimeRequirement {
                        runtime: "Python".into(),
                        requirement: requirement.into(),
                    });
                }
                collect_python_project_dependencies(&value, &mut dependencies, &mut warnings);
            }
        }
        if let Ok(source) = fs::read_to_string(directory.join("requirements.txt")) {
            collect_requirements_dependencies(&source, &mut dependencies);
        }
    }

    if let Some(declared) = package_manager.as_deref() {
        if declared.starts_with("pnpm") && markers.contains("package-lock.json") {
            warnings.push("packageManager 声明 pnpm，但目录中存在 package-lock.json。".into());
        }
        if declared.starts_with("npm") && markers.contains("pnpm-lock.yaml") {
            warnings.push("packageManager 声明 npm，但目录中存在 pnpm-lock.yaml。".into());
        }
    }
    if markers.contains("pnpm-lock.yaml") && markers.contains("package-lock.json") {
        warnings.push("同时发现 pnpm 与 npm 锁文件，请确认实际使用的包管理器。".into());
    }

    ProjectMetadata {
        name,
        path: directory.to_string_lossy().into_owned(),
        ecosystems,
        lock_files: markers
            .iter()
            .filter(|marker| {
                marker.ends_with("lock.yaml")
                    || marker.ends_with("lock.json")
                    || marker.as_str() == "uv.lock"
                    || marker.as_str() == "requirements.txt"
            })
            .cloned()
            .collect(),
        runtime_requirements,
        package_manager,
        dependencies: merge_project_dependencies(dependencies, &mut warnings),
        warnings,
    }
}

fn package_manager_from_locks(
    markers: &BTreeSet<String>,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let node_locks = [
        ("pnpm-lock.yaml", "pnpm"),
        ("package-lock.json", "npm"),
        ("yarn.lock", "yarn"),
        ("bun.lock", "bun"),
        ("bun.lockb", "bun"),
    ];
    let found = node_locks
        .iter()
        .filter(|(marker, _)| markers.contains(*marker))
        .map(|(_, manager)| *manager)
        .collect::<BTreeSet<_>>();
    if found.len() > 1 {
        warnings.push("目录中存在多个 JavaScript 锁文件，无法确定唯一包管理器。".into());
        None
    } else {
        found.into_iter().next().map(str::to_string)
    }
}

fn collect_javascript_dependencies(
    value: &JsonValue,
    dependencies: &mut Vec<ProjectDependency>,
    warnings: &mut Vec<String>,
) {
    for (key, scope) in [
        ("dependencies", "运行"),
        ("devDependencies", "开发"),
        ("optionalDependencies", "可选"),
        ("peerDependencies", "Peer"),
    ] {
        let Some(items) = value.get(key).and_then(JsonValue::as_object) else {
            continue;
        };
        for (name, requirement) in items {
            let Some(requirement) = requirement.as_str() else {
                warnings.push(format!("JavaScript 依赖 {name} 的版本声明无效。"));
                continue;
            };
            dependencies.push(project_dependency("JavaScript", name, requirement, scope));
        }
    }
}

fn collect_python_project_dependencies(
    value: &TomlValue,
    dependencies: &mut Vec<ProjectDependency>,
    warnings: &mut Vec<String>,
) {
    let Some(project) = value.get("project") else {
        return;
    };
    if let Some(items) = project.get("dependencies").and_then(TomlValue::as_array) {
        for item in items.iter().filter_map(TomlValue::as_str) {
            push_python_requirement(item, "运行", dependencies, warnings);
        }
    }
    if let Some(groups) = project
        .get("optional-dependencies")
        .and_then(TomlValue::as_table)
    {
        for (group, items) in groups {
            for item in items
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(TomlValue::as_str)
            {
                push_python_requirement(item, &format!("可选:{group}"), dependencies, warnings);
            }
        }
    }
}

fn collect_requirements_dependencies(source: &str, dependencies: &mut Vec<ProjectDependency>) {
    for line in source.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with('-') || line.contains("://") || line.starts_with('.')
        {
            continue;
        }
        let mut ignored_warnings = Vec::new();
        push_python_requirement(
            line,
            "requirements.txt",
            dependencies,
            &mut ignored_warnings,
        );
    }
}

fn push_python_requirement(
    source: &str,
    scope: &str,
    dependencies: &mut Vec<ProjectDependency>,
    warnings: &mut Vec<String>,
) {
    let declaration = source.split(';').next().unwrap_or("").trim();
    let name_end = declaration
        .char_indices()
        .find(|(_, character)| matches!(character, '<' | '>' | '=' | '!' | '~' | '@' | '[' | ' '))
        .map(|(index, _)| index)
        .unwrap_or(declaration.len());
    let name = declaration[..name_end].trim();
    if name.is_empty()
        || !name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        warnings.push(format!("Python 依赖声明无效：{source}"));
        return;
    }
    let requirement = declaration[name_end..].trim();
    if !requirement.is_empty()
        && !matches!(
            requirement.as_bytes().first(),
            Some(b'<' | b'>' | b'=' | b'!' | b'~' | b'@' | b'[')
        )
    {
        warnings.push(format!("Python 依赖声明无效：{source}"));
        return;
    }
    dependencies.push(project_dependency(
        "Python",
        name,
        if requirement.is_empty() {
            "未声明版本"
        } else {
            requirement
        },
        scope,
    ));
}

fn collect_rust_dependencies(
    value: &TomlValue,
    dependencies: &mut Vec<ProjectDependency>,
    warnings: &mut Vec<String>,
) {
    for (key, scope) in [
        ("dependencies", "运行"),
        ("dev-dependencies", "开发"),
        ("build-dependencies", "构建"),
    ] {
        let Some(items) = value.get(key).and_then(TomlValue::as_table) else {
            continue;
        };
        for (name, declaration) in items {
            let requirement = match declaration {
                TomlValue::String(value) => Some(value.as_str()),
                TomlValue::Table(value) => value.get("version").and_then(TomlValue::as_str),
                _ => None,
            };
            match requirement {
                Some(requirement) => {
                    dependencies.push(project_dependency("Rust", name, requirement, scope))
                }
                None => warnings.push(format!("Rust 依赖 {name} 未声明可识别的版本。")),
            }
        }
    }
}

fn project_dependency(
    ecosystem: &str,
    name: &str,
    version_requirement: &str,
    scope: &str,
) -> ProjectDependency {
    ProjectDependency {
        ecosystem: ecosystem.into(),
        name: name.into(),
        normalized_name: normalize_dependency_name(ecosystem, name),
        version_requirement: version_requirement.into(),
        scopes: vec![scope.into()],
    }
}

fn normalize_dependency_name(ecosystem: &str, name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if ecosystem == "Python" {
        lower.replace(['_', '.'], "-")
    } else {
        lower
    }
}

fn merge_project_dependencies(
    dependencies: Vec<ProjectDependency>,
    warnings: &mut Vec<String>,
) -> Vec<ProjectDependency> {
    let mut merged = BTreeMap::<(String, String), ProjectDependency>::new();
    for dependency in dependencies {
        let key = (
            dependency.ecosystem.clone(),
            dependency.normalized_name.clone(),
        );
        if let Some(existing) = merged.get_mut(&key) {
            if existing.version_requirement != dependency.version_requirement {
                warnings.push(format!(
                    "{} 依赖 {} 存在冲突版本声明：{} 与 {}。",
                    dependency.ecosystem,
                    dependency.name,
                    existing.version_requirement,
                    dependency.version_requirement
                ));
            }
            existing.scopes.extend(dependency.scopes);
            existing.scopes.sort();
            existing.scopes.dedup();
        } else {
            merged.insert(key, dependency);
        }
    }
    merged.into_values().collect()
}

fn dependency_insights(projects: &[ProjectMetadata]) -> Vec<DependencyInsight> {
    let mut insights = BTreeMap::<(String, String), DependencyInsight>::new();
    for project in projects {
        for dependency in &project.dependencies {
            let entry = insights
                .entry((
                    dependency.ecosystem.clone(),
                    dependency.normalized_name.clone(),
                ))
                .or_insert_with(|| DependencyInsight {
                    ecosystem: dependency.ecosystem.clone(),
                    name: dependency.name.clone(),
                    project_count: 0,
                    version_requirements: Vec::new(),
                    projects: Vec::new(),
                    has_version_divergence: false,
                });
            entry.project_count += 1;
            if !entry
                .version_requirements
                .contains(&dependency.version_requirement)
            {
                entry
                    .version_requirements
                    .push(dependency.version_requirement.clone());
            }
            entry.projects.push(DependencyProjectUsage {
                project_name: project.name.clone(),
                project_path: project.path.clone(),
                version_requirement: dependency.version_requirement.clone(),
                scopes: dependency.scopes.clone(),
            });
        }
    }
    for insight in insights.values_mut() {
        insight.version_requirements.sort();
        insight.has_version_divergence = insight.version_requirements.len() > 1;
        insight
            .projects
            .sort_by(|left, right| left.project_name.cmp(&right.project_name));
    }
    insights.into_values().collect()
}

fn project_log(status: LogStatus, message: &str) -> TaskLog {
    TaskLog {
        id: Uuid::new_v4().to_string(),
        category: LogCategory::Project,
        status,
        message: message.into(),
        manager_id: None,
        exit_code: None,
        output: None,
        timestamp: Utc::now().to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn scans_project_metadata_and_ignores_node_modules() {
        let root = tempdir().unwrap();
        let project = root.path().join("demo");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("package.json"),
            r#"{"name":"demo-app","packageManager":"pnpm@11","engines":{"node":">=22"}}"#,
        )
        .unwrap();
        fs::write(project.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'").unwrap();
        let ignored = project.join("node_modules/hidden");
        fs::create_dir_all(&ignored).unwrap();
        fs::write(ignored.join("package.json"), r#"{"name":"hidden"}"#).unwrap();
        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].name, "demo-app");
        assert_eq!(
            result.projects[0].runtime_requirements[0].requirement,
            ">=22"
        );
    }

    #[test]
    fn reports_lockfile_mismatch() {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("package.json"),
            r#"{"name":"demo","packageManager":"pnpm@11"}"#,
        )
        .unwrap();
        fs::write(root.path().join("package-lock.json"), "{}").unwrap();
        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        assert_eq!(result.projects[0].warnings.len(), 1);
    }

    #[test]
    fn reads_rust_project_metadata() {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname = \"toolbox\"\nrust-version = \"1.84\"\n",
        )
        .unwrap();
        fs::write(root.path().join("Cargo.lock"), "version = 4").unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();

        assert_eq!(result.projects[0].name, "toolbox");
        assert_eq!(result.projects[0].package_manager.as_deref(), Some("cargo"));
        assert_eq!(result.projects[0].runtime_requirements[0].runtime, "Rust");
    }

    #[test]
    fn stops_when_scan_is_cancelled() {
        let root = tempdir().unwrap();
        let cancelled = AtomicBool::new(true);

        let error = scan_projects(&[root.path().to_path_buf()], &cancelled).unwrap_err();

        assert!(matches!(error, AppError::ScanCancelled));
    }

    #[test]
    fn reports_ambiguous_node_lockfiles() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("package.json"), "{}").unwrap();
        fs::write(root.path().join("yarn.lock"), "").unwrap();
        fs::write(root.path().join("bun.lock"), "").unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();

        assert!(result.projects[0].package_manager.is_none());
        assert_eq!(result.projects[0].warnings.len(), 1);
    }

    #[test]
    fn indexes_direct_dependencies_across_ecosystems_and_scopes() {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("package.json"),
            r#"{"dependencies":{"react":"^19"},"devDependencies":{"react":"^19","vitest":"^3"}}"#,
        )
        .unwrap();
        fs::write(
            root.path().join("pyproject.toml"),
            "[project]\ndependencies = [\"HTTPX>=0.28\", \"invalid requirement!\"]\n",
        )
        .unwrap();
        fs::write(
            root.path().join("requirements.txt"),
            "pydantic>=2\n-r nested.txt\nhttps://example.com/a.whl\n",
        )
        .unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            "[dependencies]\nserde = \"1\"\n[dev-dependencies]\nserde = { version = \"1\" }\n",
        )
        .unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        let dependencies = &result.projects[0].dependencies;
        let react = dependencies
            .iter()
            .find(|item| item.name == "react")
            .unwrap();
        assert_eq!(react.scopes, vec!["开发", "运行"]);
        assert!(dependencies
            .iter()
            .any(|item| item.ecosystem == "Python" && item.normalized_name == "httpx"));
        assert!(dependencies
            .iter()
            .any(|item| item.ecosystem == "Rust" && item.name == "serde"));
        assert!(!dependencies
            .iter()
            .any(|item| item.name.contains("invalid")));
    }

    #[test]
    fn keeps_same_name_isolated_by_ecosystem_and_detects_versions() {
        let root = tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(
            first.join("package.json"),
            r#"{"dependencies":{"shared":"^1"}}"#,
        )
        .unwrap();
        fs::write(
            first.join("pyproject.toml"),
            "[project]\ndependencies = [\"shared>=2\"]",
        )
        .unwrap();
        fs::write(
            second.join("package.json"),
            r#"{"dependencies":{"shared":"^2"}}"#,
        )
        .unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        assert_eq!(result.dependency_insights.len(), 2);
        let javascript = result
            .dependency_insights
            .iter()
            .find(|item| item.ecosystem == "JavaScript")
            .unwrap();
        assert!(javascript.has_version_divergence);
        assert_eq!(javascript.project_count, 2);
    }
}
