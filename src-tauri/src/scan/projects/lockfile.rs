//! 锁文件只读解析：为直接依赖关联已解析版本，不执行任何包管理器命令。
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;
use toml::Value as TomlValue;

use super::parse::collect_go_dependencies;

use crate::models::ProjectDependency;

pub(super) fn resolve_project_dependencies(
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

pub(super) fn normalize_dependency_name(ecosystem: &str, name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if ecosystem == "Python" {
        lower.replace(['_', '.'], "-")
    } else {
        lower
    }
}

pub(super) fn merge_project_dependencies(
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
