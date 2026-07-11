mod parsers;
pub(crate) mod runner;

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::models::{
    CacheScanStatus, DiagnosticError, ExecutionTrust, LogCategory, LogStatus, ManagedPackage,
    ManagerStatus, NetworkPolicy, PackageManager, PackageManagerId, PackageScope, TaskLog,
    UpdateStatus,
};
use parsers::{
    parse_brew_packages, parse_cargo_packages, parse_npm_packages, parse_pip_packages,
    parse_pnpm_packages, parse_rubygems_packages, parse_uv_packages,
};
use runner::{
    can_execute, execution_trust, find_executable, readable_path, CommandOutput, CommandRunner,
};

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
    Cargo,
    Rubygems,
    Composer,
}

#[derive(Debug, Clone, Copy)]
enum PackageSource {
    Command(&'static [&'static str]),
    YarnGlobalDirectory,
    BunGlobalDirectory,
    ComposerInstalledJson,
}

#[derive(Debug, Clone, Copy)]
enum CacheSource {
    Command(&'static [&'static str]),
    Bun,
    Cargo,
    None,
}

#[derive(Debug, Clone)]
struct ManagerSpec {
    id: PackageManagerId,
    display_name: &'static str,
    executable_names: &'static [&'static str],
    common_paths: &'static [&'static str],
    version_args: &'static [&'static str],
    package_source: PackageSource,
    outdated_args: Option<&'static [&'static str]>,
    cache_source: CacheSource,
    scope: PackageScope,
    parser: ParserKind,
}

const SPECS: [ManagerSpec; 10] = [
    ManagerSpec {
        id: PackageManagerId::Homebrew,
        display_name: "Homebrew",
        executable_names: &["brew"],
        common_paths: &["/opt/homebrew/bin/brew", "/usr/local/bin/brew"],
        version_args: &["--version"],
        package_source: PackageSource::Command(&["list", "--versions"]),
        outdated_args: Some(&["outdated"]),
        cache_source: CacheSource::Command(&["--cache"]),
        scope: PackageScope::System,
        parser: ParserKind::Brew,
    },
    ManagerSpec {
        id: PackageManagerId::Npm,
        display_name: "npm",
        executable_names: &["npm"],
        common_paths: &[],
        version_args: &["--version"],
        package_source: PackageSource::Command(&["list", "--global", "--depth=0", "--json"]),
        outdated_args: Some(&["outdated", "--global", "--json"]),
        cache_source: CacheSource::Command(&["config", "get", "cache"]),
        scope: PackageScope::Global,
        parser: ParserKind::Npm,
    },
    ManagerSpec {
        id: PackageManagerId::Pnpm,
        display_name: "pnpm",
        executable_names: &["pnpm"],
        common_paths: &[],
        version_args: &["--version"],
        package_source: PackageSource::Command(&["list", "--global", "--depth=0", "--json"]),
        outdated_args: Some(&["outdated", "--global", "--format", "json"]),
        cache_source: CacheSource::Command(&["store", "path"]),
        scope: PackageScope::Global,
        parser: ParserKind::Pnpm,
    },
    ManagerSpec {
        id: PackageManagerId::Uv,
        display_name: "uv",
        executable_names: &["uv"],
        common_paths: &[],
        version_args: &["--version"],
        package_source: PackageSource::Command(&["tool", "list"]),
        outdated_args: None,
        cache_source: CacheSource::Command(&["cache", "dir"]),
        scope: PackageScope::Tool,
        parser: ParserKind::Uv,
    },
    ManagerSpec {
        id: PackageManagerId::Pip,
        display_name: "pip",
        executable_names: &["pip3", "pip"],
        common_paths: &["/opt/homebrew/bin/pip3", "/usr/local/bin/pip3"],
        version_args: &["--version"],
        package_source: PackageSource::Command(&[
            "list",
            "--format=json",
            "--disable-pip-version-check",
        ]),
        outdated_args: Some(&[
            "list",
            "--outdated",
            "--format=json",
            "--disable-pip-version-check",
        ]),
        cache_source: CacheSource::Command(&["cache", "dir"]),
        scope: PackageScope::Global,
        parser: ParserKind::Pip,
    },
    ManagerSpec {
        id: PackageManagerId::Yarn,
        display_name: "Yarn",
        executable_names: &["yarn"],
        common_paths: &[],
        version_args: &["--version"],
        package_source: PackageSource::YarnGlobalDirectory,
        outdated_args: None,
        cache_source: CacheSource::None,
        scope: PackageScope::Global,
        parser: ParserKind::Npm,
    },
    ManagerSpec {
        id: PackageManagerId::Bun,
        display_name: "Bun",
        executable_names: &["bun"],
        common_paths: &[],
        version_args: &["--version"],
        package_source: PackageSource::BunGlobalDirectory,
        outdated_args: None,
        cache_source: CacheSource::Bun,
        scope: PackageScope::Global,
        parser: ParserKind::Npm,
    },
    ManagerSpec {
        id: PackageManagerId::Cargo,
        display_name: "Cargo",
        executable_names: &["cargo"],
        common_paths: &[],
        version_args: &["--version"],
        package_source: PackageSource::Command(&["install", "--list"]),
        outdated_args: None,
        cache_source: CacheSource::Cargo,
        scope: PackageScope::Tool,
        parser: ParserKind::Cargo,
    },
    ManagerSpec {
        id: PackageManagerId::Rubygems,
        display_name: "RubyGems",
        executable_names: &["gem"],
        common_paths: &["/opt/homebrew/bin/gem", "/usr/local/bin/gem"],
        version_args: &["--version"],
        package_source: PackageSource::Command(&["list", "--local"]),
        outdated_args: None,
        cache_source: CacheSource::None,
        scope: PackageScope::Global,
        parser: ParserKind::Rubygems,
    },
    ManagerSpec {
        id: PackageManagerId::Composer,
        display_name: "Composer",
        executable_names: &["composer"],
        common_paths: &["/opt/homebrew/bin/composer", "/usr/local/bin/composer"],
        version_args: &["--version"],
        package_source: PackageSource::ComposerInstalledJson,
        outdated_args: None,
        cache_source: CacheSource::Command(&["config", "--global", "cache-dir"]),
        scope: PackageScope::Global,
        parser: ParserKind::Composer,
    },
];

const MAX_PARALLEL_MANAGER_SCANS: usize = 3;

pub fn scan_all(
    cancelled: &AtomicBool,
    on_complete: Arc<dyn Fn(PackageManagerId) + Send + Sync>,
    network_policy: NetworkPolicy,
) -> Result<Vec<AdapterScan>, crate::error::AppError> {
    if !cfg!(target_os = "macos") {
        return Ok(SPECS.iter().map(unsupported_scan).collect());
    }
    let workers = MAX_PARALLEL_MANAGER_SCANS.min(SPECS.len());
    let completed = Arc::new(AtomicUsize::new(0));
    let scans = thread::scope(|scope| {
        let mut handles = Vec::new();
        for worker in 0..workers {
            let on_complete = on_complete.clone();
            let completed = completed.clone();
            handles.push(scope.spawn(move || {
                let runner = CommandRunner::default();
                SPECS
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| index % workers == worker)
                    .map(|(index, spec)| {
                        let scan = scan_manager(spec, &runner, cancelled, network_policy)?;
                        let _ = completed.fetch_add(1, Ordering::SeqCst);
                        on_complete(spec.id);
                        Ok((index, scan))
                    })
                    .collect::<Result<Vec<_>, crate::error::AppError>>()
            }));
        }
        let mut scans = Vec::new();
        for handle in handles {
            scans.extend(
                handle
                    .join()
                    .map_err(|_| crate::error::AppError::Command("扫描线程异常退出".into()))??,
            );
        }
        Ok::<_, crate::error::AppError>(scans)
    })?;
    let mut scans = scans;
    scans.sort_by_key(|(index, _)| *index);
    Ok(scans.into_iter().map(|(_, scan)| scan).collect())
}

fn unsupported_scan(spec: &ManagerSpec) -> AdapterScan {
    AdapterScan {
        manager: PackageManager {
            id: spec.id,
            display_name: spec.display_name.to_string(),
            version: None,
            executable_path: None,
            status: ManagerStatus::Unsupported,
            execution_trust: ExecutionTrust::NotApplicable,
            capabilities: Vec::new(),
            error: Some(DiagnosticError {
                code: "PLATFORM_UNSUPPORTED".into(),
                message: "当前平台尚未支持该适配器".into(),
                exit_code: None,
                output: None,
            }),
            cache_size_bytes: None,
            cache_scan_status: CacheScanStatus::NotApplicable,
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

fn scan_manager(
    spec: &ManagerSpec,
    runner: &CommandRunner,
    cancelled: &AtomicBool,
    network_policy: NetworkPolicy,
) -> Result<AdapterScan, crate::error::AppError> {
    if cancelled.load(Ordering::SeqCst) {
        return Err(crate::error::AppError::ScanCancelled);
    }
    let scanned_at = Utc::now().to_rfc3339();
    let Some(executable) = find_executable(spec.executable_names, spec.common_paths) else {
        return Ok(AdapterScan {
            manager: PackageManager {
                id: spec.id,
                display_name: spec.display_name.into(),
                version: None,
                executable_path: None,
                status: ManagerStatus::Unavailable,
                execution_trust: ExecutionTrust::NotApplicable,
                capabilities: Vec::new(),
                error: None,
                cache_size_bytes: None,
                cache_scan_status: CacheScanStatus::NotApplicable,
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
        });
    };

    let trust = execution_trust(&executable, spec.common_paths);
    if !can_execute(trust) {
        let error = DiagnosticError {
            code: "UNVERIFIED_EXECUTABLE".into(),
            message: "发现未经验证的 PATH 可执行文件，已跳过执行".into(),
            exit_code: None,
            output: None,
        };
        return Ok(AdapterScan {
            manager: PackageManager {
                id: spec.id,
                display_name: spec.display_name.into(),
                version: None,
                executable_path: Some(readable_path(&executable)),
                status: ManagerStatus::Blocked,
                execution_trust: trust,
                capabilities: Vec::new(),
                error: Some(error.clone()),
                cache_size_bytes: None,
                cache_scan_status: CacheScanStatus::NotApplicable,
                scanned_at,
            },
            packages: Vec::new(),
            logs: vec![log(
                spec.id,
                LogStatus::Warning,
                "发现未经验证的 PATH 可执行文件，已跳过执行",
                Some(error),
            )],
            partial_failures: 0,
        });
    }

    let version_output = runner.run_cancellable(&executable, spec.version_args, cancelled);
    if cancelled.load(Ordering::SeqCst) {
        return Err(crate::error::AppError::ScanCancelled);
    }
    if !version_output.success {
        let error = diagnostic(
            "VERSION_COMMAND_FAILED",
            "版本命令执行失败",
            &version_output,
        );
        return Ok(AdapterScan {
            manager: PackageManager {
                id: spec.id,
                display_name: spec.display_name.into(),
                version: None,
                executable_path: Some(readable_path(&executable)),
                status: ManagerStatus::Error,
                execution_trust: trust,
                capabilities: Vec::new(),
                error: Some(error.clone()),
                cache_size_bytes: None,
                cache_scan_status: CacheScanStatus::NotApplicable,
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
        });
    }

    let version = parse_version(spec.parser, &version_output.stdout);
    let mut logs = Vec::new();
    let mut partial_failures = 0;
    let (mut packages, supports_packages) = read_packages(
        spec,
        runner,
        &executable,
        version.as_deref(),
        cancelled,
        &mut logs,
        &mut partial_failures,
    )?;

    if let Some(args) = outdated_args_for_policy(network_policy, spec) {
        let output = runner.run_cancellable(&executable, args, cancelled);
        if cancelled.load(Ordering::SeqCst) {
            return Err(crate::error::AppError::ScanCancelled);
        }
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
    } else if network_policy == NetworkPolicy::Registry {
        logs.push(log(
            spec.id,
            LogStatus::Info,
            "当前适配器不提供安全的更新检查",
            None,
        ));
    } else {
        logs.push(log(
            spec.id,
            LogStatus::Info,
            "离线策略已跳过更新检查",
            None,
        ));
    }

    let (cache_size_bytes, cache_scan_status) =
        read_cache_size(spec, runner, &executable, cancelled, &mut logs)?;

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
    Ok(AdapterScan {
        manager: PackageManager {
            id: spec.id,
            display_name: spec.display_name.into(),
            version,
            executable_path: Some(readable_path(&executable)),
            status: ManagerStatus::Available,
            execution_trust: trust,
            capabilities: manager_capabilities(spec, supports_packages, cache_size_bytes.is_some()),
            error: None,
            cache_size_bytes,
            cache_scan_status,
            scanned_at,
        },
        packages,
        logs,
        partial_failures,
    })
}

fn outdated_args_for_policy(
    network_policy: NetworkPolicy,
    spec: &ManagerSpec,
) -> Option<&'static [&'static str]> {
    (network_policy == NetworkPolicy::Registry)
        .then_some(spec.outdated_args)
        .flatten()
}

fn read_packages(
    spec: &ManagerSpec,
    runner: &CommandRunner,
    executable: &Path,
    version: Option<&str>,
    cancelled: &AtomicBool,
    logs: &mut Vec<TaskLog>,
    partial_failures: &mut usize,
) -> Result<(Vec<ManagedPackage>, bool), crate::error::AppError> {
    if cancelled.load(Ordering::SeqCst) {
        return Err(crate::error::AppError::ScanCancelled);
    }
    let values = match spec.package_source {
        PackageSource::Command(args) => {
            let output = runner.run_cancellable(executable, args, cancelled);
            if cancelled.load(Ordering::SeqCst) {
                return Err(crate::error::AppError::ScanCancelled);
            }
            if output.success {
                Some(parse_installed(spec, &output.stdout))
            } else {
                *partial_failures += 1;
                let error = diagnostic("LIST_COMMAND_FAILED", "读取已安装软件包失败", &output);
                logs.push(log(
                    spec.id,
                    LogStatus::Error,
                    "读取已安装软件包失败",
                    Some(error),
                ));
                Some(Vec::new())
            }
        }
        PackageSource::YarnGlobalDirectory => {
            if !version.is_some_and(|value| value.starts_with("1.")) {
                logs.push(log(
                    spec.id,
                    LogStatus::Info,
                    "Yarn Berry 不支持全局包扫描",
                    None,
                ));
                return Ok((Vec::new(), false));
            }
            let output = runner.run_cancellable(executable, &["global", "dir"], cancelled);
            if cancelled.load(Ordering::SeqCst) {
                return Err(crate::error::AppError::ScanCancelled);
            }
            if !output.success {
                *partial_failures += 1;
                let error = diagnostic("YARN_GLOBAL_DIR_FAILED", "无法读取 Yarn 全局目录", &output);
                logs.push(log(
                    spec.id,
                    LogStatus::Warning,
                    "无法读取 Yarn 全局目录",
                    Some(error),
                ));
                Some(Vec::new())
            } else {
                output
                    .stdout
                    .lines()
                    .map(|line| PathBuf::from(line.trim()))
                    .find(|path| path.is_dir())
                    .map(|path| packages_from_node_modules(spec, &path.join("node_modules")))
                    .transpose()?
            }
        }
        PackageSource::BunGlobalDirectory => {
            let root = bun_install_dir().join("install/global/node_modules");
            Some(packages_from_node_modules(spec, &root)?)
        }
        PackageSource::ComposerInstalledJson => Some(composer_global_packages(spec)?),
    };
    Ok((values.unwrap_or_default(), true))
}

fn packages_from_node_modules(
    spec: &ManagerSpec,
    directory: &Path,
) -> Result<Vec<ManagedPackage>, crate::error::AppError> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut package_dirs = Vec::new();
    for entry in fs::read_dir(directory)
        .map_err(|error| crate::error::AppError::Command(error.to_string()))?
    {
        let entry = entry.map_err(|error| crate::error::AppError::Command(error.to_string()))?;
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('@') {
            if let Ok(children) = fs::read_dir(&path) {
                package_dirs.extend(children.filter_map(Result::ok).map(|child| child.path()));
            }
        } else {
            package_dirs.push(path);
        }
    }
    let mut packages = Vec::new();
    for package_dir in package_dirs {
        let manifest = package_dir.join("package.json");
        let Ok(source) = fs::read_to_string(manifest) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&source) else {
            continue;
        };
        let Some(name) = value.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(version) = value.get("version").and_then(Value::as_str) else {
            continue;
        };
        packages.push(ManagedPackage {
            id: format!("{}:{name}", spec.id.as_str()),
            manager_id: spec.id,
            name: name.into(),
            version: version.into(),
            latest_version: None,
            scope: spec.scope,
            update_status: UpdateStatus::Unknown,
        });
    }
    packages.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(packages)
}

fn bun_install_dir() -> PathBuf {
    std::env::var_os("BUN_INSTALL")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".bun")))
        .unwrap_or_default()
}

fn cargo_home() -> PathBuf {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".cargo")))
        .unwrap_or_default()
}

fn composer_global_packages(
    spec: &ManagerSpec,
) -> Result<Vec<ManagedPackage>, crate::error::AppError> {
    let installed = composer_home_candidates()
        .into_iter()
        .map(|home| home.join("vendor/composer/installed.json"))
        .find(|path| path.is_file());
    let Some(installed) = installed else {
        return Ok(Vec::new());
    };
    composer_packages_from_installed(spec, &installed)
}

fn composer_packages_from_installed(
    spec: &ManagerSpec,
    installed: &Path,
) -> Result<Vec<ManagedPackage>, crate::error::AppError> {
    let source = fs::read_to_string(installed)
        .map_err(|error| crate::error::AppError::Command(error.to_string()))?;
    let mut packages = parsers::parse_composer_packages(&source)
        .into_iter()
        .map(|(name, version)| ManagedPackage {
            id: format!("{}:{name}", spec.id.as_str()),
            manager_id: spec.id,
            name,
            version,
            latest_version: None,
            scope: spec.scope,
            update_status: UpdateStatus::Unknown,
        })
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(packages)
}

fn composer_home_candidates() -> Vec<PathBuf> {
    let mut homes = Vec::new();
    if let Some(home) = std::env::var_os("COMPOSER_HOME").map(PathBuf::from) {
        homes.push(home);
    }
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
        homes.push(config.join("composer"));
    }
    if let Some(home) = dirs::home_dir() {
        homes.extend([
            home.join(".config/composer"),
            home.join("Library/Application Support/Composer"),
            home.join(".composer"),
        ]);
    }
    homes
}

fn read_cache_size(
    spec: &ManagerSpec,
    runner: &CommandRunner,
    executable: &Path,
    cancelled: &AtomicBool,
    logs: &mut Vec<TaskLog>,
) -> Result<(Option<u64>, CacheScanStatus), crate::error::AppError> {
    let path = match spec.cache_source {
        CacheSource::None => return Ok((None, CacheScanStatus::NotApplicable)),
        CacheSource::Bun => Some(bun_install_dir().join("install/cache")),
        CacheSource::Cargo => Some(cargo_home().join("registry/cache")),
        CacheSource::Command(args) => {
            let output = runner.run_cancellable(executable, args, cancelled);
            if cancelled.load(Ordering::SeqCst) {
                return Err(crate::error::AppError::ScanCancelled);
            }
            if output.success {
                output
                    .stdout
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .map(|line| PathBuf::from(line.trim()))
            } else {
                logs.push(log(
                    spec.id,
                    LogStatus::Warning,
                    "无法读取缓存目录",
                    Some(diagnostic("CACHE_PATH_FAILED", "无法读取缓存目录", &output)),
                ));
                None
            }
        }
    };
    let Some(path) = path else {
        return Ok((None, CacheScanStatus::Unavailable));
    };
    let result = directory_size(&path, cancelled)?;
    if result.status == CacheScanStatus::Partial {
        logs.push(log(
            spec.id,
            LogStatus::Warning,
            "缓存目录较大，仅展示预算范围内的统计",
            None,
        ));
    }
    Ok((Some(result.bytes), result.status))
}

fn manager_capabilities(spec: &ManagerSpec, packages: bool, cache: bool) -> Vec<String> {
    let mut capabilities = Vec::new();
    if packages {
        capabilities.push("packages".into());
    }
    if spec.outdated_args.is_some() {
        capabilities.push("outdated".into());
    }
    if cache {
        capabilities.push("cache".into());
    }
    capabilities
}

fn parse_version(kind: ParserKind, output: &str) -> Option<String> {
    let line = output.lines().find(|line| !line.trim().is_empty())?.trim();
    match kind {
        ParserKind::Brew | ParserKind::Cargo => line.split_whitespace().nth(1),
        ParserKind::Uv => line.split_whitespace().nth(1),
        ParserKind::Pip => line.split_whitespace().nth(1),
        ParserKind::Npm | ParserKind::Pnpm | ParserKind::Rubygems => line.split_whitespace().next(),
        ParserKind::Composer => line
            .split_whitespace()
            .nth(2)
            .map(|value| value.trim_start_matches('v')),
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
        ParserKind::Cargo => parse_cargo_packages(output),
        ParserKind::Rubygems => parse_rubygems_packages(output),
        ParserKind::Composer => Vec::new(),
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
        ParserKind::Uv | ParserKind::Cargo | ParserKind::Rubygems | ParserKind::Composer => {
            HashMap::new()
        }
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

const CACHE_SCAN_ENTRY_LIMIT: usize = 100_000;
const CACHE_SCAN_TIME_LIMIT: Duration = Duration::from_secs(3);

#[derive(Debug)]
struct DirectorySizeResult {
    bytes: u64,
    status: CacheScanStatus,
}

fn directory_size(
    path: &Path,
    cancelled: &AtomicBool,
) -> Result<DirectorySizeResult, crate::error::AppError> {
    if !path.exists() {
        return Ok(DirectorySizeResult {
            bytes: 0,
            status: CacheScanStatus::Complete,
        });
    }
    let started_at = Instant::now();
    let mut total = 0u64;
    let mut visited = 0usize;
    let mut partial = false;
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        if cancelled.load(Ordering::SeqCst) {
            return Err(crate::error::AppError::ScanCancelled);
        }
        if visited >= CACHE_SCAN_ENTRY_LIMIT || started_at.elapsed() >= CACHE_SCAN_TIME_LIMIT {
            partial = true;
            break;
        }
        visited += 1;
        let Ok(entry) = entry else {
            partial = true;
            continue;
        };
        if entry.file_type().is_file() {
            if let Ok(metadata) = entry.metadata() {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    Ok(DirectorySizeResult {
        bytes: total,
        status: if partial {
            CacheScanStatus::Partial
        } else {
            CacheScanStatus::Complete
        },
    })
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
        assert_eq!(
            parse_version(ParserKind::Composer, "Composer version 2.8.6 2025-02-01"),
            Some("2.8.6".into())
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

    #[test]
    fn yarn_uses_global_directory_instead_of_networked_list_command() {
        let yarn = SPECS
            .iter()
            .find(|spec| spec.id == PackageManagerId::Yarn)
            .expect("Yarn spec should exist");
        assert!(matches!(
            yarn.package_source,
            PackageSource::YarnGlobalDirectory
        ));
        assert!(yarn.outdated_args.is_none());
    }

    #[test]
    fn manager_specs_expose_only_read_only_command_arguments() {
        let forbidden = ["add", "remove", "uninstall", "upgrade", "update", "clean"];
        for spec in SPECS {
            let command_sets = [
                Some(spec.version_args),
                match spec.package_source {
                    PackageSource::Command(args) => Some(args),
                    _ => None,
                },
                spec.outdated_args,
                match spec.cache_source {
                    CacheSource::Command(args) => Some(args),
                    _ => None,
                },
            ];
            for args in command_sets.into_iter().flatten() {
                assert!(
                    !args.iter().any(|arg| forbidden.contains(arg)),
                    "{} has a mutating command argument: {args:?}",
                    spec.display_name
                );
            }
        }
        let cargo = SPECS
            .iter()
            .find(|spec| spec.id == PackageManagerId::Cargo)
            .unwrap();
        assert!(matches!(
            cargo.package_source,
            PackageSource::Command(&["install", "--list"])
        ));
    }

    #[test]
    fn bun_and_cargo_do_not_report_outdated_capability() {
        for id in [
            PackageManagerId::Bun,
            PackageManagerId::Cargo,
            PackageManagerId::Rubygems,
            PackageManagerId::Composer,
        ] {
            let spec = SPECS.iter().find(|spec| spec.id == id).unwrap();
            assert!(spec.outdated_args.is_none());
        }
    }

    #[test]
    fn offline_policy_never_runs_registry_update_checks() {
        for spec in &SPECS {
            assert!(outdated_args_for_policy(NetworkPolicy::Offline, spec).is_none());
        }
        let npm = SPECS
            .iter()
            .find(|spec| spec.id == PackageManagerId::Npm)
            .unwrap();
        assert!(outdated_args_for_policy(NetworkPolicy::Registry, npm).is_some());
    }

    #[test]
    fn cache_walk_honors_cancellation() {
        let cancelled = AtomicBool::new(true);
        let error = directory_size(Path::new("/"), &cancelled).unwrap_err();
        assert!(matches!(error, crate::error::AppError::ScanCancelled));
    }

    #[test]
    fn rubygems_and_composer_use_only_read_only_package_sources() {
        let rubygems = SPECS
            .iter()
            .find(|spec| spec.id == PackageManagerId::Rubygems)
            .unwrap();
        assert!(matches!(
            rubygems.package_source,
            PackageSource::Command(&["list", "--local"])
        ));
        assert!(matches!(rubygems.cache_source, CacheSource::None));

        let composer = SPECS
            .iter()
            .find(|spec| spec.id == PackageManagerId::Composer)
            .unwrap();
        assert!(matches!(
            composer.package_source,
            PackageSource::ComposerInstalledJson
        ));
        assert!(matches!(
            composer.cache_source,
            CacheSource::Command(&["config", "--global", "cache-dir"])
        ));
    }

    #[test]
    fn reads_bun_style_global_node_modules_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let modules = directory.path().join("node_modules/@scope/tool");
        std::fs::create_dir_all(&modules).unwrap();
        std::fs::write(
            modules.join("package.json"),
            r#"{"name":"@scope/tool","version":"1.2.3"}"#,
        )
        .unwrap();
        let bun = SPECS
            .iter()
            .find(|spec| spec.id == PackageManagerId::Bun)
            .unwrap();

        let packages =
            packages_from_node_modules(bun, &directory.path().join("node_modules")).unwrap();

        assert_eq!(packages[0].id, "bun:@scope/tool");
        assert_eq!(packages[0].version, "1.2.3");
    }

    #[test]
    fn reads_composer_global_installed_metadata_without_running_composer() {
        let directory = tempfile::tempdir().unwrap();
        let installed = directory.path().join("installed.json");
        std::fs::write(
            &installed,
            r#"{"packages":[{"name":"psr/log","version":"3.0.2"}]}"#,
        )
        .unwrap();
        let composer = SPECS
            .iter()
            .find(|spec| spec.id == PackageManagerId::Composer)
            .unwrap();

        let packages = composer_packages_from_installed(composer, &installed).unwrap();

        assert_eq!(packages[0].id, "composer:psr/log");
        assert_eq!(packages[0].version, "3.0.2");
        assert_eq!(packages[0].update_status, UpdateStatus::Unknown);
    }
}
