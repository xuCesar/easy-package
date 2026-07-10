mod parsers;
pub(crate) mod runner;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::models::{
    DiagnosticError, LogCategory, LogStatus, ManagedPackage, ManagerStatus, PackageManager,
    PackageManagerId, PackageScope, TaskLog, UpdateStatus,
};
use parsers::{
    parse_brew_packages, parse_npm_packages, parse_pip_packages, parse_pnpm_packages,
    parse_uv_packages,
};
use runner::{find_executable, readable_path, CommandOutput, CommandRunner};

#[derive(Debug)]
pub struct AdapterScan {
    pub manager: PackageManager,
    pub packages: Vec<ManagedPackage>,
    pub logs: Vec<TaskLog>,
    pub partial_failures: usize,
}

#[derive(Debug, Clone, Copy)]
enum ParserKind {
    Brew,
    Npm,
    Pnpm,
    Uv,
    Pip,
}

#[derive(Debug, Clone)]
struct ManagerSpec {
    id: PackageManagerId,
    display_name: &'static str,
    executable_names: &'static [&'static str],
    common_paths: &'static [&'static str],
    version_args: &'static [&'static str],
    list_args: &'static [&'static str],
    outdated_args: Option<&'static [&'static str]>,
    cache_args: &'static [&'static str],
    scope: PackageScope,
    parser: ParserKind,
}

const SPECS: [ManagerSpec; 5] = [
    ManagerSpec {
        id: PackageManagerId::Homebrew,
        display_name: "Homebrew",
        executable_names: &["brew"],
        common_paths: &["/opt/homebrew/bin/brew", "/usr/local/bin/brew"],
        version_args: &["--version"],
        list_args: &["list", "--versions"],
        outdated_args: Some(&["outdated"]),
        cache_args: &["--cache"],
        scope: PackageScope::System,
        parser: ParserKind::Brew,
    },
    ManagerSpec {
        id: PackageManagerId::Npm,
        display_name: "npm",
        executable_names: &["npm"],
        common_paths: &[],
        version_args: &["--version"],
        list_args: &["list", "--global", "--depth=0", "--json"],
        outdated_args: Some(&["outdated", "--global", "--json"]),
        cache_args: &["config", "get", "cache"],
        scope: PackageScope::Global,
        parser: ParserKind::Npm,
    },
    ManagerSpec {
        id: PackageManagerId::Pnpm,
        display_name: "pnpm",
        executable_names: &["pnpm"],
        common_paths: &[],
        version_args: &["--version"],
        list_args: &["list", "--global", "--depth=0", "--json"],
        outdated_args: Some(&["outdated", "--global", "--format", "json"]),
        cache_args: &["store", "path"],
        scope: PackageScope::Global,
        parser: ParserKind::Pnpm,
    },
    ManagerSpec {
        id: PackageManagerId::Uv,
        display_name: "uv",
        executable_names: &["uv"],
        common_paths: &[],
        version_args: &["--version"],
        list_args: &["tool", "list"],
        outdated_args: None,
        cache_args: &["cache", "dir"],
        scope: PackageScope::Tool,
        parser: ParserKind::Uv,
    },
    ManagerSpec {
        id: PackageManagerId::Pip,
        display_name: "pip",
        executable_names: &["pip3", "pip"],
        common_paths: &["/opt/homebrew/bin/pip3", "/usr/local/bin/pip3"],
        version_args: &["--version"],
        list_args: &["list", "--format=json", "--disable-pip-version-check"],
        outdated_args: Some(&[
            "list",
            "--outdated",
            "--format=json",
            "--disable-pip-version-check",
        ]),
        cache_args: &["cache", "dir"],
        scope: PackageScope::Global,
        parser: ParserKind::Pip,
    },
];

pub fn scan_all() -> Vec<AdapterScan> {
    if !cfg!(target_os = "macos") {
        return SPECS.iter().map(unsupported_scan).collect();
    }
    let runner = CommandRunner::default();
    SPECS
        .iter()
        .map(|spec| scan_manager(spec, &runner))
        .collect()
}

fn unsupported_scan(spec: &ManagerSpec) -> AdapterScan {
    AdapterScan {
        manager: PackageManager {
            id: spec.id,
            display_name: spec.display_name.to_string(),
            version: None,
            executable_path: None,
            status: ManagerStatus::Unsupported,
            capabilities: Vec::new(),
            error: Some(DiagnosticError {
                code: "PLATFORM_UNSUPPORTED".into(),
                message: "当前平台尚未支持该适配器".into(),
                exit_code: None,
                output: None,
            }),
            cache_size_bytes: None,
            scanned_at: Utc::now().to_rfc3339(),
        },
        packages: Vec::new(),
        logs: vec![log(
            spec.id,
            LogStatus::Warning,
            "当前平台尚未支持该适配器",
            None,
        )],
        partial_failures: 0,
    }
}

fn scan_manager(spec: &ManagerSpec, runner: &CommandRunner) -> AdapterScan {
    let scanned_at = Utc::now().to_rfc3339();
    let Some(executable) = find_executable(spec.executable_names, spec.common_paths) else {
        return AdapterScan {
            manager: PackageManager {
                id: spec.id,
                display_name: spec.display_name.into(),
                version: None,
                executable_path: None,
                status: ManagerStatus::Unavailable,
                capabilities: Vec::new(),
                error: None,
                cache_size_bytes: None,
                scanned_at,
            },
            packages: Vec::new(),
            logs: vec![log(
                spec.id,
                LogStatus::Info,
                &format!("未检测到 {}", spec.display_name),
                None,
            )],
            partial_failures: 0,
        };
    };

    let version_output = runner.run(&executable, spec.version_args);
    if !version_output.success {
        let error = diagnostic(
            "VERSION_COMMAND_FAILED",
            "版本命令执行失败",
            &version_output,
        );
        return AdapterScan {
            manager: PackageManager {
                id: spec.id,
                display_name: spec.display_name.into(),
                version: None,
                executable_path: Some(readable_path(&executable)),
                status: ManagerStatus::Error,
                capabilities: Vec::new(),
                error: Some(error.clone()),
                cache_size_bytes: None,
                scanned_at,
            },
            packages: Vec::new(),
            logs: vec![log(
                spec.id,
                LogStatus::Error,
                "版本命令执行失败",
                Some(error),
            )],
            partial_failures: 1,
        };
    }

    let version = parse_version(spec.parser, &version_output.stdout);
    let mut logs = Vec::new();
    let mut partial_failures = 0;
    let list_output = runner.run(&executable, spec.list_args);
    let mut packages = if list_output.success {
        parse_installed(spec, &list_output.stdout)
    } else {
        partial_failures += 1;
        let error = diagnostic("LIST_COMMAND_FAILED", "读取已安装软件包失败", &list_output);
        logs.push(log(
            spec.id,
            LogStatus::Error,
            "读取已安装软件包失败",
            Some(error),
        ));
        Vec::new()
    };

    if let Some(args) = spec.outdated_args {
        let output = runner.run(&executable, args);
        // npm/pnpm 在发现过期包时可能返回非零退出码，只要输出能解析就继续使用。
        if output.success || !output.stdout.trim().is_empty() {
            apply_outdated(spec.parser, &output.stdout, &mut packages);
        } else {
            partial_failures += 1;
            let error = diagnostic("OUTDATED_COMMAND_FAILED", "读取更新状态失败", &output);
            logs.push(log(
                spec.id,
                LogStatus::Warning,
                "读取更新状态失败",
                Some(error),
            ));
        }
    } else {
        logs.push(log(
            spec.id,
            LogStatus::Info,
            "当前适配器不提供安全的更新检查",
            None,
        ));
    }

    let cache_output = runner.run(&executable, spec.cache_args);
    let cache_size_bytes = if cache_output.success {
        cache_output
            .stdout
            .lines()
            .find(|line| !line.trim().is_empty())
            .map(str::trim)
            .map(PathBuf::from)
            .and_then(|path| directory_size(&path))
    } else {
        logs.push(log(
            spec.id,
            LogStatus::Warning,
            "无法读取缓存目录",
            Some(diagnostic(
                "CACHE_PATH_FAILED",
                "无法读取缓存目录",
                &cache_output,
            )),
        ));
        None
    };

    logs.push(log(
        spec.id,
        LogStatus::Success,
        &format!(
            "{} 扫描完成，共 {} 个软件包",
            spec.display_name,
            packages.len()
        ),
        None,
    ));
    AdapterScan {
        manager: PackageManager {
            id: spec.id,
            display_name: spec.display_name.into(),
            version,
            executable_path: Some(readable_path(&executable)),
            status: ManagerStatus::Available,
            capabilities: if spec.outdated_args.is_some() {
                vec!["packages".into(), "outdated".into(), "cache".into()]
            } else {
                vec!["packages".into(), "cache".into()]
            },
            error: None,
            cache_size_bytes,
            scanned_at,
        },
        packages,
        logs,
        partial_failures,
    }
}

fn parse_version(kind: ParserKind, output: &str) -> Option<String> {
    let line = output.lines().find(|line| !line.trim().is_empty())?.trim();
    match kind {
        ParserKind::Brew => line.split_whitespace().nth(1),
        ParserKind::Uv => line.split_whitespace().nth(1),
        ParserKind::Pip => line.split_whitespace().nth(1),
        ParserKind::Npm | ParserKind::Pnpm => line.split_whitespace().next(),
    }
    .map(str::to_string)
}

fn parse_installed(spec: &ManagerSpec, output: &str) -> Vec<ManagedPackage> {
    let values = match spec.parser {
        ParserKind::Brew => parse_brew_packages(output),
        ParserKind::Npm => parse_npm_packages(output),
        ParserKind::Pnpm => parse_pnpm_packages(output),
        ParserKind::Uv => parse_uv_packages(output),
        ParserKind::Pip => parse_pip_packages(output),
    };
    values
        .into_iter()
        .map(|(name, version)| ManagedPackage {
            id: format!("{}:{}", spec.id.as_str(), name),
            manager_id: spec.id,
            name,
            version,
            latest_version: None,
            scope: spec.scope,
            update_status: UpdateStatus::Unknown,
        })
        .collect()
}

fn apply_outdated(kind: ParserKind, output: &str, packages: &mut [ManagedPackage]) {
    let updates = parse_outdated(kind, output);
    for package in packages {
        if let Some(latest) = updates.get(&package.name.to_lowercase()) {
            package.latest_version = latest.clone();
            package.update_status = UpdateStatus::Available;
        } else {
            package.latest_version = Some(package.version.clone());
            package.update_status = UpdateStatus::UpToDate;
        }
    }
}

fn parse_outdated(kind: ParserKind, output: &str) -> HashMap<String, Option<String>> {
    match kind {
        ParserKind::Brew => output
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .map(|name| (name.to_lowercase(), None))
            .collect(),
        ParserKind::Npm | ParserKind::Pnpm => {
            let Ok(value) = serde_json::from_str::<Value>(output) else {
                return HashMap::new();
            };
            json_outdated_entries(&value)
        }
        ParserKind::Pip => {
            let Ok(value) = serde_json::from_str::<Value>(output) else {
                return HashMap::new();
            };
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|item| {
                    Some((
                        item.get("name")?.as_str()?.to_lowercase(),
                        item.get("latest_version")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    ))
                })
                .collect()
        }
        ParserKind::Uv => HashMap::new(),
    }
}

fn json_outdated_entries(value: &Value) -> HashMap<String, Option<String>> {
    let object = if let Some(array) = value.as_array() {
        array.first().and_then(Value::as_object)
    } else {
        value.as_object()
    };
    object
        .into_iter()
        .flatten()
        .filter_map(|(name, metadata)| {
            if name == "path" || name == "private" {
                return None;
            }
            let latest = metadata
                .get("latest")
                .or_else(|| metadata.get("latestVersion"))
                .or_else(|| metadata.get("wanted"))
                .and_then(Value::as_str)
                .map(str::to_string);
            Some((name.to_lowercase(), latest))
        })
        .collect()
}

fn diagnostic(code: &str, message: &str, output: &CommandOutput) -> DiagnosticError {
    DiagnosticError {
        code: code.into(),
        message: message.into(),
        exit_code: output.exit_code,
        output: Some(output.combined_output()),
    }
}

fn log(
    manager_id: PackageManagerId,
    status: LogStatus,
    message: &str,
    error: Option<DiagnosticError>,
) -> TaskLog {
    TaskLog {
        id: Uuid::new_v4().to_string(),
        category: LogCategory::Manager,
        status,
        message: message.into(),
        manager_id: Some(manager_id),
        exit_code: error.as_ref().and_then(|value| value.exit_code),
        output: error.and_then(|value| value.output),
        timestamp: Utc::now().to_rfc3339(),
    }
}

fn directory_size(path: &Path) -> Option<u64> {
    if !path.exists() {
        return Some(0);
    }
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() {
            if let Ok(metadata) = entry.metadata() {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manager_versions() {
        assert_eq!(
            parse_version(ParserKind::Brew, "Homebrew 4.6.15\n"),
            Some("4.6.15".into())
        );
        assert_eq!(
            parse_version(ParserKind::Pip, "pip 25.2 from /tmp/pip (python 3.13)"),
            Some("25.2".into())
        );
        assert_eq!(
            parse_version(ParserKind::Uv, "uv 0.8.13"),
            Some("0.8.13".into())
        );
    }

    #[test]
    fn applies_update_status_without_losing_installed_version() {
        let spec = &SPECS[4];
        let mut packages = parse_installed(spec, r#"[{"name":"black","version":"24.10.0"}]"#);
        apply_outdated(
            ParserKind::Pip,
            r#"[{"name":"black","version":"24.10.0","latest_version":"25.1.0"}]"#,
            &mut packages,
        );
        assert_eq!(packages[0].version, "24.10.0");
        assert_eq!(packages[0].latest_version.as_deref(), Some("25.1.0"));
        assert_eq!(packages[0].update_status, UpdateStatus::Available);
    }
}
