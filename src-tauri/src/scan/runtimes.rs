use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    adapters::runner::{
        can_execute, execution_trust, find_all_in_path_for_names, readable_path, CommandRunner,
    },
    error::AppError,
    models::{
        ProjectMetadata, RuntimeInstallation, RuntimeRequirementAssessment,
        RuntimeRequirementStatus,
    },
};

const MAX_RUNTIME_CANDIDATES: usize = 64;

#[derive(Clone, Copy)]
struct RuntimeSpec {
    runtime: &'static str,
    command_names: &'static [&'static str],
    version_args: &'static [&'static str],
}

const RUNTIME_SPECS: [RuntimeSpec; 3] = [
    RuntimeSpec {
        runtime: "Node.js",
        command_names: &["node"],
        version_args: &["--version"],
    },
    RuntimeSpec {
        runtime: "Python",
        command_names: &["python3", "python"],
        version_args: &["--version"],
    },
    RuntimeSpec {
        runtime: "Rust",
        command_names: &["rustc"],
        version_args: &["--version"],
    },
];

pub fn scan_runtimes(
    projects: &[ProjectMetadata],
    cancelled: &AtomicBool,
) -> Result<(Vec<RuntimeInstallation>, Vec<RuntimeRequirementAssessment>), AppError> {
    let runner = CommandRunner::default();
    let mut installations = Vec::new();
    for spec in RUNTIME_SPECS {
        if cancelled.load(Ordering::SeqCst) {
            return Err(AppError::ScanCancelled);
        }
        installations.extend(discover_runtime(spec, &runner, cancelled)?);
    }
    installations.sort_by(|left, right| {
        left.runtime
            .cmp(&right.runtime)
            .then_with(|| right.is_active.cmp(&left.is_active))
            .then_with(|| left.version.cmp(&right.version))
            .then_with(|| left.path.cmp(&right.path))
    });
    let assessments = assess_requirements(projects, &installations);
    Ok((installations, assessments))
}

fn discover_runtime(
    spec: RuntimeSpec,
    runner: &CommandRunner,
    cancelled: &AtomicBool,
) -> Result<Vec<RuntimeInstallation>, AppError> {
    let active = find_all_in_path_for_names(spec.command_names)
        .into_iter()
        .next();
    let active_canonical = active.as_deref().and_then(|path| path.canonicalize().ok());
    let mut candidates = active.into_iter().collect::<Vec<_>>();
    candidates.extend(known_runtime_paths(spec.runtime));

    let mut seen = HashSet::new();
    let mut installations = Vec::new();
    for path in candidates.into_iter().take(MAX_RUNTIME_CANDIDATES) {
        if cancelled.load(Ordering::SeqCst) {
            return Err(AppError::ScanCancelled);
        }
        if !path.is_file() {
            continue;
        }
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        if !seen.insert(canonical.clone()) {
            continue;
        }
        let trust = execution_trust(&path, &[]);
        let is_active = active_canonical.as_ref() == Some(&canonical);
        let version = if can_execute(trust) && is_active {
            let output = runner.run_cancellable(&path, spec.version_args, cancelled);
            if cancelled.load(Ordering::SeqCst) {
                return Err(AppError::ScanCancelled);
            }
            parse_runtime_version(spec.runtime, &output.stdout, &output.stderr)
                .unwrap_or_else(|| "未知".into())
        } else {
            inferred_version(&canonical, spec.runtime).unwrap_or_else(|| "未知".into())
        };
        installations.push(RuntimeInstallation {
            id: format!(
                "{}:{}",
                runtime_key(spec.runtime),
                readable_path(&canonical)
            ),
            runtime: spec.runtime.into(),
            version,
            path: readable_path(&canonical),
            provider: provider_for_path(&canonical),
            is_active,
            execution_trust: trust,
        });
    }
    Ok(installations)
}

fn known_runtime_paths(runtime: &str) -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let layouts: Vec<(PathBuf, &str)> = match runtime {
        "Node.js" => vec![
            (home.join(".nvm/versions/node"), "bin/node"),
            (home.join(".volta/tools/image/node"), "bin/node"),
            (home.join(".asdf/installs/nodejs"), "bin/node"),
            (home.join(".local/share/mise/installs/node"), "bin/node"),
            (home.join(".fnm/node-versions"), "installation/bin/node"),
            (
                home.join(".local/share/fnm/node-versions"),
                "installation/bin/node",
            ),
        ],
        "Python" => vec![
            (home.join(".pyenv/versions"), "bin/python"),
            (home.join(".asdf/installs/python"), "bin/python"),
            (home.join(".local/share/mise/installs/python"), "bin/python"),
            (home.join(".local/share/uv/python"), "bin/python3"),
        ],
        "Rust" => vec![(home.join(".rustup/toolchains"), "bin/rustc")],
        _ => Vec::new(),
    };
    layouts
        .into_iter()
        .flat_map(|(root, suffix)| child_executables(&root, suffix))
        .collect()
}

fn child_executables(root: &Path, suffix: &str) -> Vec<PathBuf> {
    fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(suffix))
        .filter(|path| path.is_file())
        .collect()
}

fn parse_runtime_version(runtime: &str, stdout: &str, stderr: &str) -> Option<String> {
    let value = if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    };
    let line = value.lines().find(|line| !line.trim().is_empty())?.trim();
    match runtime {
        "Node.js" => Some(line.trim_start_matches('v').to_string()),
        "Python" | "Rust" => line.split_whitespace().nth(1).map(str::to_string),
        _ => None,
    }
}

fn inferred_version(path: &Path, runtime: &str) -> Option<String> {
    let parts = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    let bin_index = parts.iter().rposition(|part| *part == "bin")?;
    let raw = match runtime {
        "Node.js" if bin_index >= 2 && parts[bin_index - 1] == "installation" => {
            parts[bin_index - 2]
        }
        "Node.js" | "Python" | "Rust" if bin_index >= 1 => parts[bin_index - 1],
        _ => return None,
    };
    let value = raw.trim_start_matches('v').trim_start_matches("cpython-");
    let semantic_prefix = value
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect::<String>();
    if semantic_prefix.matches('.').count() >= 1 {
        Some(semantic_prefix.trim_end_matches('.').to_string())
    } else {
        (!value.is_empty()).then(|| value.to_string())
    }
}

fn provider_for_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    [
        ("/.nvm/", "nvm"),
        ("/.fnm/", "fnm"),
        ("/.local/share/fnm/", "fnm"),
        ("/.volta/", "Volta"),
        ("/.asdf/", "asdf"),
        ("/.local/share/mise/", "mise"),
        ("/.pyenv/", "pyenv"),
        ("/.local/share/uv/", "uv"),
        ("/.rustup/", "rustup"),
        ("/opt/homebrew/", "Homebrew"),
        ("/usr/local/", "usr-local"),
        ("/usr/bin/", "macOS"),
    ]
    .into_iter()
    .find_map(|(needle, provider)| value.contains(needle).then(|| provider.to_string()))
    .unwrap_or_else(|| "PATH".into())
}

pub fn assess_requirements(
    projects: &[ProjectMetadata],
    installations: &[RuntimeInstallation],
) -> Vec<RuntimeRequirementAssessment> {
    let mut assessments = Vec::new();
    for project in projects {
        for requirement in &project.runtime_requirements {
            let Some(runtime) = supported_runtime(&requirement.runtime) else {
                continue;
            };
            let matching = installations
                .iter()
                .filter(|installation| installation.runtime == runtime)
                .collect::<Vec<_>>();
            let installed_versions = matching
                .iter()
                .map(|installation| installation.version.clone())
                .filter(|version| version != "未知")
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let active_version = matching
                .iter()
                .find(|installation| installation.is_active && installation.version != "未知")
                .map(|installation| installation.version.clone());
            let exact = exact_requirement(&requirement.requirement);
            let (status, message) = if matching.is_empty() {
                (
                    RuntimeRequirementStatus::Missing,
                    format!("未发现可用的 {runtime} 运行时"),
                )
            } else if let (Some(exact), Some(active)) =
                (exact.as_deref(), active_version.as_deref())
            {
                if active == exact {
                    (
                        RuntimeRequirementStatus::Available,
                        "当前激活版本与精确声明一致".into(),
                    )
                } else {
                    (
                        RuntimeRequirementStatus::Mismatch,
                        format!("当前激活版本 {active} 与精确声明 {exact} 不一致"),
                    )
                }
            } else {
                (
                    RuntimeRequirementStatus::Available,
                    "已发现本机运行时；复杂版本范围未自动判定".into(),
                )
            };
            assessments.push(RuntimeRequirementAssessment {
                project_name: project.name.clone(),
                project_path: project.path.clone(),
                runtime: runtime.into(),
                requirement: requirement.requirement.clone(),
                status,
                active_version,
                installed_versions,
                message,
            });
        }
    }
    assessments.sort_by(|left, right| {
        left.runtime
            .cmp(&right.runtime)
            .then_with(|| left.project_path.cmp(&right.project_path))
    });
    assessments
}

fn supported_runtime(value: &str) -> Option<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "node" | "node.js" | "nodejs" => Some("Node.js"),
        "python" => Some("Python"),
        "rust" => Some("Rust"),
        _ => None,
    }
}

fn runtime_key(runtime: &str) -> &'static str {
    match runtime {
        "Node.js" => "node",
        "Python" => "python",
        "Rust" => "rust",
        _ => "runtime",
    }
}

fn exact_requirement(value: &str) -> Option<String> {
    let value = value.trim().trim_start_matches(['v', '=']);
    (value.matches('.').count() >= 2
        && value
            .split('.')
            .all(|segment| !segment.is_empty() && segment.chars().all(|ch| ch.is_ascii_digit())))
    .then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ProjectMetadata, RuntimeRequirement};

    fn project(runtime: &str, requirement: &str) -> ProjectMetadata {
        ProjectMetadata {
            name: "demo".into(),
            path: "/tmp/demo".into(),
            ecosystems: vec![],
            lock_files: vec![],
            runtime_requirements: vec![RuntimeRequirement {
                runtime: runtime.into(),
                requirement: requirement.into(),
            }],
            package_manager: None,
            dependencies: vec![],
            workspace: None,
            warnings: vec![],
        }
    }

    #[test]
    fn parses_supported_runtime_versions() {
        assert_eq!(
            parse_runtime_version("Node.js", "v22.17.1\n", ""),
            Some("22.17.1".into())
        );
        assert_eq!(
            parse_runtime_version("Python", "Python 3.13.5\n", ""),
            Some("3.13.5".into())
        );
        assert_eq!(
            parse_runtime_version("Rust", "rustc 1.88.0 (abc)\n", ""),
            Some("1.88.0".into())
        );
        assert_eq!(
            inferred_version(
                Path::new("/Users/demo/.nvm/versions/node/v20.19.4/bin/node"),
                "Node.js"
            ),
            Some("20.19.4".into())
        );
    }

    #[test]
    fn assesses_missing_and_exact_runtime_mismatch() {
        let missing = assess_requirements(&[project("Python", ">=3.12")], &[]);
        assert_eq!(missing[0].status, RuntimeRequirementStatus::Missing);

        let installations = vec![RuntimeInstallation {
            id: "node:test".into(),
            runtime: "Node.js".into(),
            version: "22.17.0".into(),
            path: "/opt/homebrew/bin/node".into(),
            provider: "Homebrew".into(),
            is_active: true,
            execution_trust: crate::models::ExecutionTrust::Managed,
        }];
        let mismatch = assess_requirements(&[project("Node.js", "22.18.0")], &installations);
        assert_eq!(mismatch[0].status, RuntimeRequirementStatus::Mismatch);
    }

    #[test]
    fn leaves_complex_ranges_as_available_without_guessing_compatibility() {
        let installations = vec![RuntimeInstallation {
            id: "python:test".into(),
            runtime: "Python".into(),
            version: "3.13.5".into(),
            path: "/usr/bin/python3".into(),
            provider: "macOS".into(),
            is_active: true,
            execution_trust: crate::models::ExecutionTrust::System,
        }];
        let assessment = assess_requirements(&[project("Python", ">=3.12")], &installations);
        assert_eq!(assessment[0].status, RuntimeRequirementStatus::Available);
        assert!(assessment[0].message.contains("未自动判定"));
    }
}
