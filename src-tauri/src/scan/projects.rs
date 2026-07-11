use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use chrono::Utc;
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;
use toml::Value as TomlValue;
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use crate::{
    error::AppError,
    models::{
        DependencyInsight, DependencyProjectUsage, LogCategory, LogStatus, ProjectAnalysis,
        ProjectDependency, ProjectMetadata, ProjectWorkspace, ProjectWorkspaceRef,
        RuntimeRequirement, ScanSettings, TaskLog,
    },
};

const MARKERS: [&str; 21] = [
    "package.json",
    "pyproject.toml",
    "requirements.txt",
    "Pipfile",
    "Pipfile.lock",
    "poetry.lock",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "package-lock.json",
    "uv.lock",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
    "Cargo.toml",
    "Cargo.lock",
    "go.mod",
    "go.sum",
    "Gemfile",
    "Gemfile.lock",
    "composer.json",
    "composer.lock",
];
const MIN_MAX_DEPTH: usize = 1;
const MAX_MAX_DEPTH: usize = 12;

#[derive(Debug)]
pub struct ProjectScan {
    pub projects: Vec<ProjectMetadata>,
    pub dependency_insights: Vec<DependencyInsight>,
    pub workspaces: Vec<ProjectWorkspace>,
    pub logs: Vec<TaskLog>,
    pub failures: usize,
}

#[cfg(test)]
pub fn scan_projects(roots: &[PathBuf], cancelled: &AtomicBool) -> Result<ProjectScan, AppError> {
    scan_projects_with_settings(roots, &ScanSettings::default(), cancelled)
}

pub fn scan_projects_with_settings(
    roots: &[PathBuf],
    settings: &ScanSettings,
    cancelled: &AtomicBool,
) -> Result<ProjectScan, AppError> {
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
        let skipped = RefCell::new(BTreeSet::new());
        for entry in WalkDir::new(root)
            .max_depth(settings.max_depth)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| should_visit(entry, settings, &skipped))
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
        for message in skipped.into_inner() {
            logs.push(project_log(LogStatus::Info, &message));
        }
    }

    let mut projects = directories
        .into_iter()
        .map(|(directory, markers)| parse_project(&directory, &markers))
        .collect::<Vec<_>>();
    let workspaces = discover_workspaces(&mut projects);
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
        workspaces,
        logs,
        failures,
    })
}

pub fn analyze_projects(
    roots: &[PathBuf],
    settings: &ScanSettings,
    cancelled: &AtomicBool,
) -> Result<ProjectAnalysis, AppError> {
    let scan = scan_projects_with_settings(roots, settings, cancelled)?;
    Ok(ProjectAnalysis {
        projects: scan.projects,
        dependency_insights: scan.dependency_insights,
        workspaces: scan.workspaces,
        scan_settings: settings.clone(),
    })
}

fn discover_workspaces(projects: &mut [ProjectMetadata]) -> Vec<ProjectWorkspace> {
    let candidates = projects
        .iter()
        .flat_map(|project| {
            workspace_candidates(project)
                .into_iter()
                .map(move |candidate| (project.path.clone(), candidate))
        })
        .collect::<Vec<_>>();
    let mut workspaces = Vec::new();

    for (path, candidate) in candidates {
        let members = projects
            .iter()
            .filter(|project| {
                workspace_matches(&candidate, Path::new(&path), Path::new(&project.path))
            })
            .map(|project| project.path.clone())
            .collect::<Vec<_>>();
        if members.is_empty() {
            continue;
        }
        let workspace = ProjectWorkspace {
            name: candidate.name.clone(),
            path: path.clone(),
            ecosystem: candidate.ecosystem.clone(),
            member_paths: members.clone(),
        };
        for project in projects
            .iter_mut()
            .filter(|project| members.contains(&project.path))
        {
            project.workspace = Some(ProjectWorkspaceRef {
                name: workspace.name.clone(),
                path: workspace.path.clone(),
                ecosystem: workspace.ecosystem.clone(),
            });
        }
        workspaces.push(workspace);
    }
    workspaces.sort_by(|left, right| left.path.cmp(&right.path));
    workspaces
}

#[derive(Debug)]
struct WorkspaceCandidate {
    name: String,
    ecosystem: String,
    members: Vec<String>,
}

fn workspace_candidates(project: &ProjectMetadata) -> Vec<WorkspaceCandidate> {
    let directory = Path::new(&project.path);
    let mut candidates = Vec::new();
    if project
        .ecosystems
        .iter()
        .any(|ecosystem| ecosystem == "JavaScript")
    {
        if let Ok(source) = fs::read_to_string(directory.join("package.json")) {
            if let Ok(value) = serde_json::from_str::<JsonValue>(&source) {
                let members = match value.get("workspaces") {
                    Some(JsonValue::Array(items)) => items
                        .iter()
                        .filter_map(JsonValue::as_str)
                        .map(str::to_string)
                        .collect(),
                    Some(JsonValue::Object(value)) => value
                        .get("packages")
                        .and_then(JsonValue::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(JsonValue::as_str)
                        .map(str::to_string)
                        .collect(),
                    _ => Vec::new(),
                };
                if !members.is_empty() {
                    candidates.push(WorkspaceCandidate {
                        name: project.name.clone(),
                        ecosystem: "JavaScript".into(),
                        members,
                    });
                }
            }
        }
        if !candidates
            .iter()
            .any(|candidate| candidate.ecosystem == "JavaScript")
        {
            if let Ok(source) = fs::read_to_string(directory.join("pnpm-workspace.yaml")) {
                let members = source
                    .lines()
                    .map(str::trim)
                    .filter_map(|line| line.strip_prefix('-'))
                    .map(str::trim)
                    .map(|line| line.trim_matches(['\'', '"']).to_string())
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>();
                if !members.is_empty() {
                    candidates.push(WorkspaceCandidate {
                        name: project.name.clone(),
                        ecosystem: "JavaScript".into(),
                        members,
                    });
                }
            }
        }
    }
    if project
        .ecosystems
        .iter()
        .any(|ecosystem| ecosystem == "Rust")
    {
        if let Ok(source) = fs::read_to_string(directory.join("Cargo.toml")) {
            if let Ok(value) = source.parse::<TomlValue>() {
                let members = value
                    .get("workspace")
                    .and_then(|workspace| workspace.get("members"))
                    .and_then(TomlValue::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(TomlValue::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                if !members.is_empty() {
                    candidates.push(WorkspaceCandidate {
                        name: project.name.clone(),
                        ecosystem: "Rust".into(),
                        members,
                    });
                }
            }
        }
    }
    candidates
}

fn workspace_matches(candidate: &WorkspaceCandidate, root: &Path, project: &Path) -> bool {
    let Ok(relative) = project.strip_prefix(root) else {
        return false;
    };
    let relative = relative.to_string_lossy().replace('\\', "/");
    candidate.members.iter().any(|pattern| {
        let normalized = pattern.trim_end_matches('/');
        if let Some(prefix) = normalized.strip_suffix("/*") {
            relative.starts_with(&format!("{prefix}/"))
                && !relative[prefix.len() + 1..].contains('/')
        } else {
            relative == normalized
        }
    })
}

fn should_visit(
    entry: &DirEntry,
    settings: &ScanSettings,
    skipped: &RefCell<BTreeSet<String>>,
) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    if !entry.file_type().is_dir() {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    if settings
        .default_ignored_directory_names
        .iter()
        .any(|ignored| ignored == name.as_ref())
    {
        skipped.borrow_mut().insert(format!(
            "跳过默认忽略目录：{}（{}）",
            name,
            entry.path().display()
        ));
        return false;
    }
    if let Some(ignored) = settings
        .ignored_paths
        .iter()
        .find(|ignored| entry.path().starts_with(Path::new(ignored)))
    {
        skipped
            .borrow_mut()
            .insert(format!("跳过用户忽略目录：{}", ignored));
        return false;
    }
    true
}

pub fn normalize_scan_settings(
    mut settings: ScanSettings,
    roots: &[PathBuf],
) -> Result<ScanSettings, AppError> {
    if !(MIN_MAX_DEPTH..=MAX_MAX_DEPTH).contains(&settings.max_depth) {
        return Err(AppError::InvalidScanSettings(format!(
            "最大扫描深度需在 {MIN_MAX_DEPTH} 到 {MAX_MAX_DEPTH} 之间"
        )));
    }
    settings.default_ignored_directory_names = crate::models::default_ignored_directory_names();
    let mut ignored_paths = BTreeSet::new();
    for raw_path in settings.ignored_paths {
        let path = PathBuf::from(raw_path.trim());
        if raw_path.trim().is_empty() || !path.is_absolute() || !path.is_dir() {
            return Err(AppError::InvalidScanSettings(
                "忽略目录必须是存在的绝对目录".into(),
            ));
        }
        let canonical = path
            .canonicalize()
            .map_err(|error| AppError::InvalidScanSettings(error.to_string()))?;
        if !roots.iter().any(|root| canonical.starts_with(root)) {
            return Err(AppError::InvalidScanSettings(
                "忽略目录必须位于已添加的扫描目录内".into(),
            ));
        }
        if roots.iter().any(|root| canonical == *root) {
            return Err(AppError::InvalidScanSettings(
                "不能将扫描根目录本身设为忽略目录".into(),
            ));
        }
        ignored_paths.insert(canonical.to_string_lossy().into_owned());
    }
    settings.ignored_paths = ignored_paths.into_iter().collect();
    Ok(settings)
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

    if markers.contains("package.json") || markers.contains("pnpm-workspace.yaml") {
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
        if markers.contains("pnpm-workspace.yaml") && package_manager.is_none() {
            package_manager = Some("pnpm workspace".into());
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

    if markers.contains("pyproject.toml")
        || markers.contains("requirements.txt")
        || markers.contains("Pipfile")
        || markers.contains("Pipfile.lock")
        || markers.contains("poetry.lock")
    {
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
                    if let Some(project_name) = value
                        .get("tool")
                        .and_then(|tool| tool.get("poetry"))
                        .and_then(|poetry| poetry.get("name"))
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
                collect_poetry_dependencies(&value, &mut dependencies, &mut warnings);
            }
        }
        if let Ok(source) = fs::read_to_string(directory.join("requirements.txt")) {
            collect_requirements_dependencies(&source, &mut dependencies);
        }
        if let Ok(source) = fs::read_to_string(directory.join("Pipfile")) {
            if let Ok(value) = source.parse::<TomlValue>() {
                collect_pipenv_dependencies(&value, &mut dependencies, &mut warnings);
                if package_manager.is_none() {
                    package_manager = Some("pipenv".into());
                }
            }
        }
        if markers.contains("poetry.lock") && package_manager.is_none() {
            package_manager = Some("poetry".into());
        }
    }

    if markers.contains("go.mod") {
        ecosystems.push("Go".into());
        if let Ok(source) = fs::read_to_string(directory.join("go.mod")) {
            let go_project = collect_go_dependencies(&source, &mut dependencies);
            if !markers.contains("package.json") && !markers.contains("Cargo.toml") {
                if let Some(module) = go_project.module_name {
                    name = module;
                }
            }
            if let Some(version) = go_project.go_version {
                runtime_requirements.push(RuntimeRequirement {
                    runtime: "Go".into(),
                    requirement: version,
                });
            }
        }
        if package_manager.is_none() {
            package_manager = Some("go".into());
        }
    }

    if markers.contains("Gemfile") || markers.contains("Gemfile.lock") {
        ecosystems.push("Ruby".into());
        if let Ok(source) = fs::read_to_string(directory.join("Gemfile")) {
            collect_ruby_dependencies(&source, &mut dependencies, &mut warnings);
        }
        if let Ok(version) = fs::read_to_string(directory.join(".ruby-version")) {
            let version = version.trim();
            if !version.is_empty() {
                runtime_requirements.push(RuntimeRequirement {
                    runtime: "Ruby".into(),
                    requirement: version.into(),
                });
            }
        }
        if package_manager.is_none() {
            package_manager = Some("bundler".into());
        }
    }

    if markers.contains("composer.json") || markers.contains("composer.lock") {
        ecosystems.push("PHP".into());
        if let Ok(value) = fs::read_to_string(directory.join("composer.json")).and_then(|value| {
            serde_json::from_str::<JsonValue>(&value).map_err(std::io::Error::other)
        }) {
            if !markers.contains("package.json") && !markers.contains("Cargo.toml") {
                if let Some(project_name) = value.get("name").and_then(JsonValue::as_str) {
                    name = project_name.into();
                }
            }
            if let Some(requirement) = value.pointer("/require/php").and_then(JsonValue::as_str) {
                runtime_requirements.push(RuntimeRequirement {
                    runtime: "PHP".into(),
                    requirement: requirement.into(),
                });
            }
            collect_composer_dependencies(&value, &mut dependencies, &mut warnings);
        }
        if package_manager.is_none() {
            package_manager = Some("composer".into());
        }
    }

    if let Some(declared) = package_manager.as_deref() {
        let expected_lock = [
            ("pnpm", "pnpm-lock.yaml"),
            ("npm", "package-lock.json"),
            ("yarn", "yarn.lock"),
            ("bun", "bun.lock"),
        ]
        .into_iter()
        .find(|(manager, _)| declared.starts_with(manager));
        let node_locks_present = [
            "pnpm-lock.yaml",
            "package-lock.json",
            "yarn.lock",
            "bun.lock",
            "bun.lockb",
        ]
        .into_iter()
        .any(|lock| markers.contains(lock));
        if let Some((manager, lock)) = expected_lock {
            let has_expected_lock =
                markers.contains(lock) || (manager == "bun" && markers.contains("bun.lockb"));
            if node_locks_present && !has_expected_lock {
                warnings.push(format!(
                    "packageManager 声明 {manager}，但目录中未发现对应的 {lock}。"
                ));
            }
        }
    }
    if markers.contains("pnpm-lock.yaml") && markers.contains("package-lock.json") {
        warnings.push("同时发现 pnpm 与 npm 锁文件，请确认实际使用的包管理器。".into());
    }
    if markers.contains("bun.lockb") && !markers.contains("bun.lock") {
        warnings.push(
            "检测到 bun.lockb 二进制锁文件；当前仅识别该文件，无法关联直接依赖的已解析版本。"
                .into(),
        );
    }

    let project_name = name.clone();
    ProjectMetadata {
        name,
        path: directory.to_string_lossy().into_owned(),
        ecosystems,
        lock_files: markers
            .iter()
            .filter(|marker| {
                marker.ends_with("lock.yaml")
                    || marker.ends_with("lock.json")
                    || marker.ends_with(".lock")
                    || marker.as_str() == "bun.lockb"
                    || matches!(marker.as_str(), "go.mod" | "go.sum")
                    || marker.as_str() == "requirements.txt"
            })
            .cloned()
            .collect(),
        runtime_requirements,
        package_manager,
        dependencies: resolve_project_dependencies(
            directory,
            &project_name,
            merge_project_dependencies(dependencies, &mut warnings),
        ),
        workspace: None,
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

fn collect_poetry_dependencies(
    value: &TomlValue,
    dependencies: &mut Vec<ProjectDependency>,
    warnings: &mut Vec<String>,
) {
    let Some(poetry) = value.get("tool").and_then(|tool| tool.get("poetry")) else {
        return;
    };
    if let Some(items) = poetry.get("dependencies").and_then(TomlValue::as_table) {
        collect_toml_dependencies(
            items,
            "Python",
            "运行",
            dependencies,
            warnings,
            Some("python"),
        );
    }
    if let Some(groups) = poetry.get("group").and_then(TomlValue::as_table) {
        for (group, definition) in groups {
            if let Some(items) = definition.get("dependencies").and_then(TomlValue::as_table) {
                collect_toml_dependencies(
                    items,
                    "Python",
                    &format!("开发:{group}"),
                    dependencies,
                    warnings,
                    None,
                );
            }
        }
    }
}

fn collect_pipenv_dependencies(
    value: &TomlValue,
    dependencies: &mut Vec<ProjectDependency>,
    warnings: &mut Vec<String>,
) {
    for (table, scope) in [("packages", "运行"), ("dev-packages", "开发")] {
        if let Some(items) = value.get(table).and_then(TomlValue::as_table) {
            collect_toml_dependencies(items, "Python", scope, dependencies, warnings, None);
        }
    }
}

fn collect_toml_dependencies(
    items: &toml::map::Map<String, TomlValue>,
    ecosystem: &str,
    scope: &str,
    dependencies: &mut Vec<ProjectDependency>,
    warnings: &mut Vec<String>,
    ignored_name: Option<&str>,
) {
    for (name, declaration) in items {
        if ignored_name.is_some_and(|ignored| name.eq_ignore_ascii_case(ignored)) {
            continue;
        }
        let requirement = match declaration {
            TomlValue::String(value) if value != "*" => Some(value.as_str()),
            TomlValue::String(_) => Some("未声明版本"),
            TomlValue::Table(value) => value
                .get("version")
                .and_then(TomlValue::as_str)
                .filter(|value| *value != "*")
                .or(Some("未声明版本")),
            _ => None,
        };
        match requirement {
            Some(requirement) => {
                dependencies.push(project_dependency(ecosystem, name, requirement, scope))
            }
            None => warnings.push(format!("{ecosystem} 依赖 {name} 的版本声明无效。")),
        }
    }
}

#[derive(Default)]
struct GoProjectMetadata {
    module_name: Option<String>,
    go_version: Option<String>,
}

fn collect_go_dependencies(
    source: &str,
    dependencies: &mut Vec<ProjectDependency>,
) -> GoProjectMetadata {
    let mut metadata = GoProjectMetadata::default();
    let mut in_require_block = false;
    for line in source.lines() {
        if line.contains("// indirect") {
            continue;
        }
        let line = line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(module) = line.strip_prefix("module ") {
            metadata.module_name = Some(module.trim().into());
            continue;
        }
        if let Some(version) = line.strip_prefix("go ") {
            metadata.go_version = Some(version.trim().into());
            continue;
        }
        if line == "require (" {
            in_require_block = true;
            continue;
        }
        if in_require_block && line == ")" {
            in_require_block = false;
            continue;
        }
        let declaration = line.strip_prefix("require ").unwrap_or(line);
        if !in_require_block && !line.starts_with("require ") {
            continue;
        }
        let mut parts = declaration.split_whitespace();
        let (Some(name), Some(version)) = (parts.next(), parts.next()) else {
            continue;
        };
        dependencies.push(project_dependency("Go", name, version, "运行"));
    }
    metadata
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

fn collect_ruby_dependencies(
    source: &str,
    dependencies: &mut Vec<ProjectDependency>,
    warnings: &mut Vec<String>,
) {
    for line in source.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some(declaration) = line.strip_prefix("gem") else {
            continue;
        };
        if !declaration
            .chars()
            .next()
            .is_some_and(|character| character.is_whitespace() || character == '(')
        {
            continue;
        }
        let declaration = declaration
            .trim_start()
            .trim_start_matches('(')
            .trim_start();
        let Some((name, remainder)) = take_quoted_value(declaration) else {
            warnings.push(format!("Ruby 依赖声明无效：{line}"));
            continue;
        };
        let remainder = remainder.trim_start();
        let requirement = remainder
            .strip_prefix(',')
            .and_then(|value| take_quoted_value(value.trim_start()).map(|(value, _)| value))
            .unwrap_or("未声明版本");
        let scope =
            if remainder.contains("group: :development") || remainder.contains("group: :test") {
                "开发"
            } else {
                "运行"
            };
        dependencies.push(project_dependency("Ruby", name, requirement, scope));
    }
}

fn take_quoted_value(source: &str) -> Option<(&str, &str)> {
    let quote = source.chars().next()?;
    if !matches!(quote, '\'' | '"') {
        return None;
    }
    let rest = &source[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some((&rest[..end], &rest[end + quote.len_utf8()..]))
}

fn collect_composer_dependencies(
    value: &JsonValue,
    dependencies: &mut Vec<ProjectDependency>,
    warnings: &mut Vec<String>,
) {
    for (key, scope) in [("require", "运行"), ("require-dev", "开发")] {
        let Some(items) = value.get(key).and_then(JsonValue::as_object) else {
            continue;
        };
        for (name, declaration) in items {
            if name == "php" || name.starts_with("ext-") || name.starts_with("lib-") {
                continue;
            }
            let Some(requirement) = declaration.as_str() else {
                warnings.push(format!("PHP 依赖 {name} 的版本声明无效。"));
                continue;
            };
            dependencies.push(project_dependency(
                "PHP",
                name,
                if requirement == "*" {
                    "未声明版本"
                } else {
                    requirement
                },
                scope,
            ));
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
        resolved_version: None,
        resolution_source: None,
        resolution_checked: false,
    }
}

fn resolve_project_dependencies(
    directory: &Path,
    project_name: &str,
    mut dependencies: Vec<ProjectDependency>,
) -> Vec<ProjectDependency> {
    apply_resolutions(
        &mut dependencies,
        "JavaScript",
        find_lock_file(directory, "package-lock.json")
            .as_deref()
            .map(resolve_npm_lock),
        "package-lock.json",
    );
    apply_resolutions(
        &mut dependencies,
        "JavaScript",
        find_lock_file(directory, "pnpm-lock.yaml")
            .as_deref()
            .map(|path| resolve_pnpm_lock(path, directory)),
        "pnpm-lock.yaml",
    );
    apply_resolutions(
        &mut dependencies,
        "JavaScript",
        find_lock_file(directory, "yarn.lock")
            .as_deref()
            .map(resolve_yarn_lock),
        "yarn.lock",
    );
    apply_resolutions(
        &mut dependencies,
        "JavaScript",
        find_lock_file(directory, "bun.lock")
            .as_deref()
            .map(resolve_bun_lock),
        "bun.lock",
    );
    apply_resolutions(
        &mut dependencies,
        "Rust",
        find_lock_file(directory, "Cargo.lock")
            .as_deref()
            .map(|path| resolve_cargo_lock(path, project_name)),
        "Cargo.lock",
    );
    apply_resolutions(
        &mut dependencies,
        "Python",
        find_lock_file(directory, "poetry.lock")
            .as_deref()
            .map(resolve_poetry_lock),
        "poetry.lock",
    );
    apply_resolutions(
        &mut dependencies,
        "Python",
        find_lock_file(directory, "Pipfile.lock")
            .as_deref()
            .map(resolve_pipfile_lock),
        "Pipfile.lock",
    );
    apply_resolutions(
        &mut dependencies,
        "Python",
        find_lock_file(directory, "uv.lock")
            .as_deref()
            .map(|path| resolve_uv_lock(path, project_name)),
        "uv.lock",
    );
    apply_resolutions(
        &mut dependencies,
        "Go",
        find_lock_file(directory, "go.mod")
            .as_deref()
            .map(resolve_go_mod),
        "go.mod",
    );
    apply_resolutions(
        &mut dependencies,
        "Ruby",
        find_lock_file(directory, "Gemfile.lock")
            .as_deref()
            .map(resolve_gemfile_lock),
        "Gemfile.lock",
    );
    apply_resolutions(
        &mut dependencies,
        "PHP",
        find_lock_file(directory, "composer.lock")
            .as_deref()
            .map(resolve_composer_lock),
        "composer.lock",
    );
    dependencies
}

fn apply_resolutions(
    dependencies: &mut [ProjectDependency],
    ecosystem: &str,
    resolutions: Option<BTreeMap<String, String>>,
    source: &str,
) {
    let Some(resolutions) = resolutions else {
        return;
    };
    for dependency in dependencies
        .iter_mut()
        .filter(|dependency| dependency.ecosystem == ecosystem)
    {
        dependency.resolution_checked = true;
        if dependency.resolved_version.is_none() {
            if let Some(version) = resolutions.get(&dependency.normalized_name) {
                dependency.resolved_version = Some(version.clone());
                dependency.resolution_source = Some(source.into());
            }
        }
    }
}

fn find_lock_file(directory: &Path, name: &str) -> Option<PathBuf> {
    directory
        .ancestors()
        .map(|path| path.join(name))
        .find(|path| path.is_file())
}

fn resolve_npm_lock(path: &Path) -> BTreeMap<String, String> {
    let Ok(source) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = serde_json::from_str::<JsonValue>(&source) else {
        return BTreeMap::new();
    };
    value
        .get("packages")
        .and_then(JsonValue::as_object)
        .map(|packages| {
            packages
                .iter()
                .filter_map(|(key, package)| {
                    let name = key.strip_prefix("node_modules/")?;
                    let version = package.get("version")?.as_str()?;
                    Some((
                        normalize_dependency_name("JavaScript", name),
                        version.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn resolve_pnpm_lock(path: &Path, project_directory: &Path) -> BTreeMap<String, String> {
    let Ok(source) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = serde_yaml::from_str::<YamlValue>(&source) else {
        return BTreeMap::new();
    };
    let importer_key = path
        .parent()
        .and_then(|root| project_directory.strip_prefix(root).ok())
        .map(|path| path.to_string_lossy().replace('\\', "/"));
    let importers = value.get("importers").and_then(YamlValue::as_mapping);
    let importer = importers.and_then(|items| {
        items.get(YamlValue::String(".".into())).or_else(|| {
            importer_key
                .as_ref()
                .and_then(|key| items.get(YamlValue::String(key.clone())))
        })
    });
    let Some(importer) = importer else {
        return BTreeMap::new();
    };
    ["dependencies", "devDependencies", "optionalDependencies"]
        .into_iter()
        .flat_map(|scope| {
            importer
                .get(scope)
                .and_then(YamlValue::as_mapping)
                .into_iter()
                .flatten()
        })
        .filter_map(|(name, declaration)| {
            let version = declaration.as_str().map(str::to_string).or_else(|| {
                declaration
                    .get("version")
                    .and_then(YamlValue::as_str)
                    .map(str::to_string)
            })?;
            let version = version.split('(').next().unwrap_or(&version).to_string();
            if version.starts_with("link:") || version.starts_with("workspace:") {
                return None;
            }
            Some((
                normalize_dependency_name("JavaScript", name.as_str()?),
                version,
            ))
        })
        .collect()
}

fn resolve_yarn_lock(path: &Path) -> BTreeMap<String, String> {
    let Ok(source) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let mut resolutions = BTreeMap::new();
    let mut selectors = Vec::new();

    for line in source.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line
            .chars()
            .next()
            .is_some_and(|character| !character.is_whitespace())
            && line.ends_with(':')
        {
            selectors = line[..line.len() - 1]
                .split(',')
                .filter_map(yarn_selector_name)
                .collect();
            continue;
        }
        let trimmed = line.trim();
        let version = trimmed
            .strip_prefix("version ")
            .or_else(|| trimmed.strip_prefix("version:"))
            .map(|value| value.trim().trim_matches(['\'', '"']));
        let Some(version) = version.filter(|value| !value.is_empty()) else {
            continue;
        };
        for name in &selectors {
            resolutions.insert(name.clone(), version.into());
        }
    }
    resolutions
}

fn yarn_selector_name(selector: &str) -> Option<String> {
    let selector = selector.trim().trim_matches(['\'', '"']);
    if selector.starts_with('@') {
        let slash = selector.find('/')?;
        let version = selector[slash + 1..].find('@')? + slash + 1;
        Some(normalize_dependency_name(
            "JavaScript",
            &selector[..version],
        ))
    } else {
        let version = selector.find('@')?;
        Some(normalize_dependency_name(
            "JavaScript",
            &selector[..version],
        ))
    }
}

fn resolve_bun_lock(path: &Path) -> BTreeMap<String, String> {
    let Ok(source) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = serde_json::from_str::<JsonValue>(&source) else {
        return BTreeMap::new();
    };
    value
        .get("packages")
        .and_then(JsonValue::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(name, package)| {
            let source = package
                .get("version")
                .and_then(JsonValue::as_str)
                .or_else(|| {
                    package
                        .as_array()
                        .and_then(|items| items.first())
                        .and_then(JsonValue::as_str)
                })
                .or_else(|| package.as_str())?;
            let version = bun_package_version(name, source)?;
            Some((
                normalize_dependency_name("JavaScript", name),
                version.into(),
            ))
        })
        .collect()
}

fn bun_package_version<'a>(name: &str, source: &'a str) -> Option<&'a str> {
    source
        .strip_prefix(name)
        .and_then(|value| value.strip_prefix('@'))
        .filter(|version| !version.is_empty() && !version.contains(':'))
}

fn resolve_cargo_lock(path: &Path, project_name: &str) -> BTreeMap<String, String> {
    let Ok(source) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = source.parse::<TomlValue>() else {
        return BTreeMap::new();
    };
    let empty_packages = Vec::new();
    let packages = value
        .get("package")
        .and_then(TomlValue::as_array)
        .unwrap_or(&empty_packages);
    let versions = packages
        .iter()
        .filter_map(|package| {
            Some((
                package.get("name")?.as_str()?,
                package.get("version")?.as_str()?,
            ))
        })
        .map(|(name, version)| (normalize_dependency_name("Rust", name), version.to_string()))
        .collect::<BTreeMap<_, _>>();
    let direct = packages
        .iter()
        .find(|package| package.get("name").and_then(TomlValue::as_str) == Some(project_name))
        .and_then(|package| package.get("dependencies"))
        .and_then(TomlValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(TomlValue::as_str)
        .filter_map(|entry| entry.split_whitespace().next())
        .map(|name| normalize_dependency_name("Rust", name))
        .collect::<BTreeSet<_>>();
    direct
        .into_iter()
        .filter_map(|name| versions.get(&name).cloned().map(|version| (name, version)))
        .collect()
}

fn resolve_uv_lock(path: &Path, project_name: &str) -> BTreeMap<String, String> {
    let Ok(source) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = source.parse::<TomlValue>() else {
        return BTreeMap::new();
    };
    let empty_packages = Vec::new();
    let packages = value
        .get("package")
        .and_then(TomlValue::as_array)
        .unwrap_or(&empty_packages);
    let versions = packages
        .iter()
        .filter_map(|package| {
            Some((
                package.get("name")?.as_str()?,
                package.get("version")?.as_str()?,
            ))
        })
        .map(|(name, version)| {
            (
                normalize_dependency_name("Python", name),
                version.to_string(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let direct = packages
        .iter()
        .find(|package| package.get("name").and_then(TomlValue::as_str) == Some(project_name))
        .and_then(|package| package.get("dependencies"))
        .and_then(TomlValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(|dependency| dependency.get("name").and_then(TomlValue::as_str))
        .map(|name| normalize_dependency_name("Python", name))
        .collect::<BTreeSet<_>>();
    direct
        .into_iter()
        .filter_map(|name| versions.get(&name).cloned().map(|version| (name, version)))
        .collect()
}

fn resolve_poetry_lock(path: &Path) -> BTreeMap<String, String> {
    let Ok(source) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = source.parse::<TomlValue>() else {
        return BTreeMap::new();
    };
    value
        .get("package")
        .and_then(TomlValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(|package| {
            Some((
                package.get("name")?.as_str()?,
                package.get("version")?.as_str()?,
            ))
        })
        .map(|(name, version)| {
            (
                normalize_dependency_name("Python", name),
                version.to_string(),
            )
        })
        .collect()
}

fn resolve_pipfile_lock(path: &Path) -> BTreeMap<String, String> {
    let Ok(source) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = serde_json::from_str::<JsonValue>(&source) else {
        return BTreeMap::new();
    };
    ["default", "develop"]
        .into_iter()
        .flat_map(|scope| {
            value
                .get(scope)
                .and_then(JsonValue::as_object)
                .into_iter()
                .flatten()
        })
        .filter_map(|(name, declaration)| {
            let version = declaration
                .get("version")?
                .as_str()?
                .trim_start_matches("==");
            Some((
                normalize_dependency_name("Python", name),
                version.to_string(),
            ))
        })
        .collect()
}

fn resolve_go_mod(path: &Path) -> BTreeMap<String, String> {
    let Ok(source) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let mut dependencies = Vec::new();
    collect_go_dependencies(&source, &mut dependencies);
    dependencies
        .into_iter()
        .map(|dependency| (dependency.normalized_name, dependency.version_requirement))
        .collect()
}

fn resolve_gemfile_lock(path: &Path) -> BTreeMap<String, String> {
    let Ok(source) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let mut versions = BTreeMap::new();
    let mut direct = BTreeSet::new();
    let mut section = "";
    let mut in_specs = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if !line.starts_with(' ') && !trimmed.is_empty() {
            section = trimmed;
            in_specs = false;
            continue;
        }
        if section == "GEM" && trimmed == "specs:" {
            in_specs = true;
            continue;
        }
        if section == "GEM" && in_specs && line.starts_with("    ") {
            if let Some((name, version)) = parse_lockfile_package(trimmed) {
                versions.insert(normalize_dependency_name("Ruby", name), version.into());
            }
        }
        if section == "DEPENDENCIES" && line.starts_with("  ") {
            if let Some(name) = trimmed.split_whitespace().next() {
                direct.insert(normalize_dependency_name(
                    "Ruby",
                    name.trim_end_matches('!'),
                ));
            }
        }
    }

    direct
        .into_iter()
        .filter_map(|name| versions.get(&name).cloned().map(|version| (name, version)))
        .collect()
}

fn parse_lockfile_package(entry: &str) -> Option<(&str, &str)> {
    let (name, version) = entry.split_once(" (")?;
    Some((name, version.strip_suffix(')')?))
}

fn resolve_composer_lock(path: &Path) -> BTreeMap<String, String> {
    let Ok(source) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = serde_json::from_str::<JsonValue>(&source) else {
        return BTreeMap::new();
    };
    ["packages", "packages-dev"]
        .into_iter()
        .flat_map(|key| {
            value
                .get(key)
                .and_then(JsonValue::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|package| {
            let name = package.get("name")?.as_str()?;
            let version = package
                .get("pretty_version")
                .or_else(|| package.get("version"))
                .and_then(JsonValue::as_str)?;
            Some((normalize_dependency_name("PHP", name), version.into()))
        })
        .collect()
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
                    resolved_versions: Vec::new(),
                    projects: Vec::new(),
                    has_version_divergence: false,
                    has_resolved_version_divergence: false,
                    has_resolution_risk: false,
                    has_health_risk: false,
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
                resolved_version: dependency.resolved_version.clone(),
                resolution_source: dependency.resolution_source.clone(),
            });
            if let Some(version) = &dependency.resolved_version {
                if !entry.resolved_versions.contains(version) {
                    entry.resolved_versions.push(version.clone());
                }
            }
            if dependency.resolution_checked && dependency.resolved_version.is_none() {
                entry.has_resolution_risk = true;
            }
        }
    }
    for insight in insights.values_mut() {
        insight.version_requirements.sort();
        insight.resolved_versions.sort();
        insight.has_version_divergence = insight.version_requirements.len() > 1;
        insight.has_resolved_version_divergence = insight.resolved_versions.len() > 1;
        insight.has_health_risk = insight.has_version_divergence
            || insight.has_resolved_version_divergence
            || insight.has_resolution_risk
            || insight
                .version_requirements
                .iter()
                .any(|requirement| is_dependency_risk(requirement));
        insight
            .projects
            .sort_by(|left, right| left.project_name.cmp(&right.project_name));
    }
    insights.into_values().collect()
}

fn is_dependency_risk(requirement: &str) -> bool {
    requirement == "未声明版本"
        || requirement.starts_with("workspace:")
        || requirement.starts_with("file:")
        || requirement.starts_with("link:")
        || requirement.starts_with("path:")
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
        assert!(result
            .logs
            .iter()
            .any(|log| log.message.contains("跳过默认忽略目录：node_modules")));
    }

    #[test]
    fn honors_user_ignored_paths_and_reports_them_in_logs() {
        let root = tempdir().unwrap();
        let ignored = root.path().join("generated");
        fs::create_dir_all(&ignored).unwrap();
        fs::write(ignored.join("package.json"), r#"{"name":"ignored"}"#).unwrap();
        let settings = ScanSettings {
            ignored_paths: vec![ignored.to_string_lossy().into_owned()],
            ..ScanSettings::default()
        };

        let result = scan_projects_with_settings(
            &[root.path().to_path_buf()],
            &settings,
            &AtomicBool::new(false),
        )
        .unwrap();

        assert!(result.projects.is_empty());
        assert!(result
            .logs
            .iter()
            .any(|log| log.message.contains("跳过用户忽略目录")));
    }

    #[test]
    fn max_depth_limits_project_marker_discovery() {
        let root = tempdir().unwrap();
        let deep_project = root.path().join("one/two/three/four/five/six");
        fs::create_dir_all(&deep_project).unwrap();
        fs::write(deep_project.join("package.json"), r#"{"name":"deep"}"#).unwrap();

        let shallow = ScanSettings {
            max_depth: 6,
            ..ScanSettings::default()
        };
        assert!(scan_projects_with_settings(
            &[root.path().to_path_buf()],
            &shallow,
            &AtomicBool::new(false),
        )
        .unwrap()
        .projects
        .is_empty());

        let deeper = ScanSettings {
            max_depth: 7,
            ..ScanSettings::default()
        };
        assert_eq!(
            scan_projects_with_settings(
                &[root.path().to_path_buf()],
                &deeper,
                &AtomicBool::new(false),
            )
            .unwrap()
            .projects
            .len(),
            1
        );
    }

    #[test]
    fn rejects_ignored_paths_outside_scan_roots_and_invalid_depth() {
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let error = normalize_scan_settings(
            ScanSettings {
                ignored_paths: vec![outside.path().to_string_lossy().into_owned()],
                ..ScanSettings::default()
            },
            &[root.path().to_path_buf()],
        )
        .unwrap_err();
        assert!(matches!(error, AppError::InvalidScanSettings(_)));

        let error = normalize_scan_settings(
            ScanSettings {
                max_depth: 0,
                ..ScanSettings::default()
            },
            &[root.path().to_path_buf()],
        )
        .unwrap_err();
        assert!(matches!(error, AppError::InvalidScanSettings(_)));
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
    fn reports_yarn_and_bun_lockfile_mismatches() {
        let root = tempdir().unwrap();
        let yarn = root.path().join("yarn-project");
        let bun = root.path().join("bun-project");
        fs::create_dir_all(&yarn).unwrap();
        fs::create_dir_all(&bun).unwrap();
        fs::write(yarn.join("package.json"), r#"{"packageManager":"yarn@4"}"#).unwrap();
        fs::write(yarn.join("package-lock.json"), "{}").unwrap();
        fs::write(bun.join("package.json"), r#"{"packageManager":"bun@1"}"#).unwrap();
        fs::write(bun.join("yarn.lock"), "").unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        assert!(result.projects.iter().all(|project| project
            .warnings
            .iter()
            .any(|warning| warning.contains("未发现对应"))));
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

    #[test]
    fn recognizes_javascript_and_rust_workspaces() {
        let root = tempdir().unwrap();
        let javascript_member = root.path().join("apps/web");
        let rust_member = root.path().join("crates/cli");
        fs::create_dir_all(&javascript_member).unwrap();
        fs::create_dir_all(&rust_member).unwrap();
        fs::write(
            root.path().join("package.json"),
            r#"{"name":"web-suite","workspaces":["apps/*"]}"#,
        )
        .unwrap();
        fs::write(javascript_member.join("package.json"), r#"{"name":"web"}"#).unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
        fs::write(
            rust_member.join("Cargo.toml"),
            "[package]\nname = \"cli\"\nversion = \"0.1.0\"",
        )
        .unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        assert_eq!(result.workspaces.len(), 2);
        assert_eq!(
            result
                .projects
                .iter()
                .find(|project| project.name == "web")
                .unwrap()
                .workspace
                .as_ref()
                .unwrap()
                .name,
            "web-suite"
        );
        assert_eq!(
            result
                .projects
                .iter()
                .find(|project| project.name == "cli")
                .unwrap()
                .workspace
                .as_ref()
                .unwrap()
                .ecosystem,
            "Rust"
        );
    }

    #[test]
    fn recognizes_pnpm_workspace_file_without_root_package_manifest() {
        let root = tempdir().unwrap();
        let member = root.path().join("packages/ui");
        fs::create_dir_all(&member).unwrap();
        fs::write(
            root.path().join("pnpm-workspace.yaml"),
            "packages:\n  - 'packages/*'\n",
        )
        .unwrap();
        fs::write(member.join("package.json"), r#"{"name":"ui"}"#).unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        assert_eq!(result.workspaces.len(), 1);
        assert_eq!(
            result.workspaces[0].member_paths,
            vec![member.to_string_lossy()]
        );
    }

    #[test]
    fn resolves_direct_dependencies_from_supported_lockfiles() {
        let root = tempdir().unwrap();
        let npm = root.path().join("npm");
        let pnpm = root.path().join("pnpm");
        let cargo = root.path().join("cargo");
        let uv = root.path().join("uv");
        for directory in [&npm, &pnpm, &cargo, &uv] {
            fs::create_dir_all(directory).unwrap();
        }
        fs::write(
            npm.join("package.json"),
            r#"{"name":"npm-app","dependencies":{"react":"^19"}}"#,
        )
        .unwrap();
        fs::write(npm.join("package-lock.json"), r#"{"lockfileVersion":3,"packages":{"":{"name":"npm-app","dependencies":{"react":"^19"}},"node_modules/react":{"version":"19.1.1"}}}"#).unwrap();
        fs::write(
            pnpm.join("package.json"),
            r#"{"name":"pnpm-app","dependencies":{"zod":"^3"}}"#,
        )
        .unwrap();
        fs::write(pnpm.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\nimporters:\n  .:\n    dependencies:\n      zod:\n        version: 3.24.1\n").unwrap();
        fs::write(
            cargo.join("Cargo.toml"),
            "[package]\nname = \"cargo-app\"\nversion = \"0.1.0\"\n[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();
        fs::write(cargo.join("Cargo.lock"), "[[package]]\nname = \"cargo-app\"\nversion = \"0.1.0\"\ndependencies = [\"serde 1.0.218\"]\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.218\"\n").unwrap();
        fs::write(
            uv.join("pyproject.toml"),
            "[project]\nname = \"uv-app\"\ndependencies = [\"httpx>=0.28\"]\n",
        )
        .unwrap();
        fs::write(uv.join("uv.lock"), "[[package]]\nname = \"uv-app\"\nversion = \"0.1.0\"\ndependencies = [{ name = \"httpx\" }]\n\n[[package]]\nname = \"httpx\"\nversion = \"0.28.1\"\n").unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        let expected = [
            ("npm-app", "react", "19.1.1", "package-lock.json"),
            ("pnpm-app", "zod", "3.24.1", "pnpm-lock.yaml"),
            ("cargo-app", "serde", "1.0.218", "Cargo.lock"),
            ("uv-app", "httpx", "0.28.1", "uv.lock"),
        ];
        for (project_name, dependency_name, version, source) in expected {
            let dependency = result
                .projects
                .iter()
                .find(|project| project.name == project_name)
                .unwrap()
                .dependencies
                .iter()
                .find(|dependency| dependency.name == dependency_name)
                .unwrap();
            assert_eq!(dependency.resolved_version.as_deref(), Some(version));
            assert_eq!(dependency.resolution_source.as_deref(), Some(source));
        }
    }

    #[test]
    fn resolves_yarn_and_bun_text_lockfiles_without_parsing_bun_lockb() {
        let root = tempdir().unwrap();
        let yarn_classic = root.path().join("yarn-classic");
        let yarn_berry = root.path().join("yarn-berry");
        let yarn_workspace = root.path().join("yarn-workspace");
        let yarn_member = yarn_workspace.join("packages/web");
        let bun = root.path().join("bun");
        let bun_binary = root.path().join("bun-binary");
        for directory in [&yarn_classic, &yarn_berry, &yarn_member, &bun, &bun_binary] {
            fs::create_dir_all(directory).unwrap();
        }
        fs::write(
            yarn_classic.join("package.json"),
            r#"{"name":"classic","dependencies":{"lodash":"^4.17.0"}}"#,
        )
        .unwrap();
        fs::write(
            yarn_classic.join("yarn.lock"),
            "# yarn lockfile v1\n\nlodash@^4.17.0:\n  version \"4.17.21\"\n",
        )
        .unwrap();
        fs::write(
            yarn_berry.join("package.json"),
            r#"{"name":"berry","packageManager":"yarn@4.6.0","dependencies":{"react":"^19.0.0"}}"#,
        )
        .unwrap();
        fs::write(
            yarn_berry.join("yarn.lock"),
            "__metadata:\n  version: 8\n\n\"react@npm:^19.0.0\":\n  version: 19.1.1\n",
        )
        .unwrap();
        fs::write(
            yarn_workspace.join("package.json"),
            r#"{"name":"suite","packageManager":"yarn@4.6.0","workspaces":["packages/*"]}"#,
        )
        .unwrap();
        fs::write(
            yarn_workspace.join("yarn.lock"),
            "\"zod@npm:^3.0.0\":\n  version: 3.24.1\n",
        )
        .unwrap();
        fs::write(
            yarn_member.join("package.json"),
            r#"{"name":"web","dependencies":{"zod":"^3.0.0"}}"#,
        )
        .unwrap();
        fs::write(
            bun.join("package.json"),
            r#"{"name":"bun-app","packageManager":"bun@1.3.0","dependencies":{"hono":"^4.6.0"}}"#,
        )
        .unwrap();
        fs::write(
            bun.join("bun.lock"),
            r#"{"lockfileVersion":0,"packages":{"hono":["hono@4.6.14","",{},""]}}"#,
        )
        .unwrap();
        fs::write(
            bun_binary.join("package.json"),
            r#"{"name":"bun-binary","packageManager":"bun@1.0.0","dependencies":{"zod":"^3.0.0"}}"#,
        )
        .unwrap();
        fs::write(bun_binary.join("bun.lockb"), [0_u8, 1, 2, 3]).unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        let resolved = [
            ("classic", "lodash", "4.17.21", "yarn.lock"),
            ("berry", "react", "19.1.1", "yarn.lock"),
            ("web", "zod", "3.24.1", "yarn.lock"),
            ("bun-app", "hono", "4.6.14", "bun.lock"),
        ];
        for (project_name, dependency_name, version, source) in resolved {
            let dependency = result
                .projects
                .iter()
                .find(|project| project.name == project_name)
                .unwrap()
                .dependencies
                .iter()
                .find(|dependency| dependency.name == dependency_name)
                .unwrap();
            assert_eq!(dependency.resolved_version.as_deref(), Some(version));
            assert_eq!(dependency.resolution_source.as_deref(), Some(source));
        }

        let binary = result
            .projects
            .iter()
            .find(|project| project.name == "bun-binary")
            .unwrap();
        assert!(binary.lock_files.contains(&"bun.lockb".into()));
        assert!(binary
            .warnings
            .iter()
            .any(|warning| warning.contains("bun.lockb 二进制")));
        assert!(!binary.dependencies[0].resolution_checked);
    }

    #[test]
    fn resolves_pnpm_workspace_importer_and_reports_unresolved_dependencies() {
        let root = tempdir().unwrap();
        let member = root.path().join("packages/web");
        fs::create_dir_all(&member).unwrap();
        fs::write(
            root.path().join("pnpm-workspace.yaml"),
            "packages:\n  - packages/*\n",
        )
        .unwrap();
        fs::write(
            member.join("package.json"),
            r#"{"name":"web","dependencies":{"react":"^19","missing":"^1"}}"#,
        )
        .unwrap();
        fs::write(root.path().join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\nimporters:\n  packages/web:\n    dependencies:\n      react:\n        version: 19.1.1\n").unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        let web = result
            .projects
            .iter()
            .find(|project| project.name == "web")
            .unwrap();
        assert_eq!(
            web.dependencies
                .iter()
                .find(|dependency| dependency.name == "react")
                .unwrap()
                .resolved_version
                .as_deref(),
            Some("19.1.1")
        );
        assert!(
            web.dependencies
                .iter()
                .find(|dependency| dependency.name == "missing")
                .unwrap()
                .resolution_checked
        );
        assert!(
            result
                .dependency_insights
                .iter()
                .find(|insight| insight.name == "missing")
                .unwrap()
                .has_resolution_risk
        );
    }

    #[test]
    fn resolves_poetry_pipenv_and_go_project_dependencies() {
        let root = tempdir().unwrap();
        let poetry = root.path().join("poetry");
        let pipenv = root.path().join("pipenv");
        let go = root.path().join("go");
        for directory in [&poetry, &pipenv, &go] {
            fs::create_dir_all(directory).unwrap();
        }
        fs::write(
            poetry.join("pyproject.toml"),
            "[tool.poetry]\nname = \"poetry-app\"\n[tool.poetry.dependencies]\npython = \">=3.12\"\nhttpx = \"^0.28\"\n[tool.poetry.group.dev.dependencies]\npytest = \"^8\"\n",
        )
        .unwrap();
        fs::write(
            poetry.join("poetry.lock"),
            "[[package]]\nname = \"httpx\"\nversion = \"0.28.1\"\n\n[[package]]\nname = \"pytest\"\nversion = \"8.3.4\"\n",
        )
        .unwrap();
        fs::write(
            pipenv.join("Pipfile"),
            "[packages]\nrequests = \"==2.32.3\"\n[dev-packages]\nblack = \"*\"\n",
        )
        .unwrap();
        fs::write(
            pipenv.join("Pipfile.lock"),
            r#"{"default":{"requests":{"version":"==2.32.3"}},"develop":{"black":{"version":"==24.10.0"}}}"#,
        )
        .unwrap();
        fs::write(
            go.join("go.mod"),
            "module example.com/tool\ngo 1.23\nrequire (\n  github.com/spf13/cobra v1.8.1\n  golang.org/x/text v0.20.0 // indirect\n)\n",
        )
        .unwrap();
        fs::write(go.join("go.sum"), "").unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        let poetry_project = result
            .projects
            .iter()
            .find(|project| project.name == "poetry-app")
            .unwrap();
        assert_eq!(
            poetry_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "httpx")
                .unwrap()
                .resolved_version
                .as_deref(),
            Some("0.28.1")
        );
        assert_eq!(
            poetry_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "pytest")
                .unwrap()
                .scopes,
            vec!["开发:dev"]
        );
        let pipenv_project = result
            .projects
            .iter()
            .find(|project| project.path == pipenv.to_string_lossy())
            .unwrap();
        assert_eq!(
            pipenv_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "black")
                .unwrap()
                .resolved_version
                .as_deref(),
            Some("24.10.0")
        );
        let go_project = result
            .projects
            .iter()
            .find(|project| project.name == "example.com/tool")
            .unwrap();
        assert_eq!(
            go_project
                .runtime_requirements
                .iter()
                .find(|item| item.runtime == "Go")
                .unwrap()
                .requirement,
            "1.23"
        );
        assert_eq!(
            go_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "github.com/spf13/cobra")
                .unwrap()
                .resolution_source
                .as_deref(),
            Some("go.mod")
        );
    }

    #[test]
    fn resolves_ruby_and_php_direct_dependencies_from_lockfiles() {
        let root = tempdir().unwrap();
        let ruby = root.path().join("ruby");
        let php = root.path().join("php");
        fs::create_dir_all(&ruby).unwrap();
        fs::create_dir_all(&php).unwrap();
        fs::write(
            ruby.join("Gemfile"),
            "gem \"rails\", \"~> 8.0\"\ngem \"rspec-rails\", \"~> 7.1\", group: :development\ngem \"missing-gem\", \"~> 1.0\"\n",
        )
        .unwrap();
        fs::write(ruby.join(".ruby-version"), "3.4.1\n").unwrap();
        fs::write(
            ruby.join("Gemfile.lock"),
            "GEM\n  specs:\n    rails (8.0.1)\n    rspec-rails (7.1.1)\n\nDEPENDENCIES\n  rails (~> 8.0)\n  rspec-rails (~> 7.1)\n",
        )
        .unwrap();
        fs::write(
            php.join("composer.json"),
            r#"{"name":"acme/api","require":{"php":"^8.3","symfony/http-foundation":"^7.2","ext-json":"*"},"require-dev":{"phpunit/phpunit":"^11.5","lib-icu":"*"}}"#,
        )
        .unwrap();
        fs::write(
            php.join("composer.lock"),
            r#"{"packages":[{"name":"symfony/http-foundation","version":"v7.2.1"}],"packages-dev":[{"name":"phpunit/phpunit","version":"11.5.3"}]}"#,
        )
        .unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        let ruby_project = result
            .projects
            .iter()
            .find(|project| project.path == ruby.to_string_lossy())
            .unwrap();
        assert_eq!(ruby_project.package_manager.as_deref(), Some("bundler"));
        assert_eq!(
            ruby_project
                .runtime_requirements
                .iter()
                .find(|item| item.runtime == "Ruby")
                .unwrap()
                .requirement,
            "3.4.1"
        );
        assert_eq!(
            ruby_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "rails")
                .unwrap()
                .resolved_version
                .as_deref(),
            Some("8.0.1")
        );
        assert_eq!(
            ruby_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "rspec-rails")
                .unwrap()
                .scopes,
            vec!["开发"]
        );
        assert!(
            ruby_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "missing-gem")
                .unwrap()
                .resolution_checked
        );

        let php_project = result
            .projects
            .iter()
            .find(|project| project.name == "acme/api")
            .unwrap();
        assert_eq!(php_project.package_manager.as_deref(), Some("composer"));
        assert_eq!(
            php_project
                .runtime_requirements
                .iter()
                .find(|item| item.runtime == "PHP")
                .unwrap()
                .requirement,
            "^8.3"
        );
        assert_eq!(php_project.dependencies.len(), 2);
        assert_eq!(
            php_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "symfony/http-foundation")
                .unwrap()
                .resolution_source
                .as_deref(),
            Some("composer.lock")
        );
        assert_eq!(
            php_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "phpunit/phpunit")
                .unwrap()
                .scopes,
            vec!["开发"]
        );
    }
}
