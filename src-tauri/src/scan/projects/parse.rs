//! 单个项目目录的 manifest 解析与各生态直接依赖收集。
use std::{collections::BTreeSet, fs, path::Path};

use serde_json::Value as JsonValue;
use toml::Value as TomlValue;

use super::lockfile::{
    merge_project_dependencies, normalize_dependency_name, resolve_project_dependencies,
};

use crate::models::{ProjectDependency, ProjectMetadata, RuntimeRequirement};

pub(super) fn parse_project(directory: &Path, markers: &BTreeSet<String>) -> ProjectMetadata {
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
        dependency_graph_summary: None,
        supply_chain_risk_summary: None,
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
pub(super) struct GoProjectMetadata {
    module_name: Option<String>,
    go_version: Option<String>,
}

pub(super) fn collect_go_dependencies(
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
