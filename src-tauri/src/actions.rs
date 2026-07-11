use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

use chrono::Utc;
use uuid::Uuid;

mod capabilities;
pub mod catalog;
mod reconcile;

pub use capabilities::package_action_capabilities;
pub use reconcile::{reconcile_observed_outcome, reconcile_pending_audits};

use crate::{
    adapters::runner::{execution_trust, redact_and_truncate, CommandRunner},
    error::AppError,
    models::{
        ExecutionTrust, ManagerStatus, PackageAction, PackageActionPlan, PackageActionStatus,
        PackageManagerId,
    },
    storage::Storage,
};

const PLAN_TTL_SECONDS: i64 = 10 * 60;
const MAX_BATCH_TARGETS: usize = 20;
const MAX_PROGRESS_LINES: usize = 200;
const MAX_ACTION_LOG_BYTES: usize = 32_000;
const ACTION_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone)]
pub struct RegisteredPlan {
    pub plan: PackageActionPlan,
    pub executable: PathBuf,
    pub args: Vec<String>,
    environment: Vec<(String, String)>,
    executable_fingerprint: Option<ExecutableFingerprint>,
    npm_context: Option<NpmExecutionContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExecutableFingerprint {
    canonical_path: PathBuf,
    size: u64,
    modified_at: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NpmExecutionContext {
    node_fingerprint: ExecutableFingerprint,
    prefix: PathBuf,
    cache: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ActionExecution {
    pub status: PackageActionStatus,
    pub logs: Vec<String>,
    pub error: Option<String>,
}

trait PackageActionRunner {
    fn run(
        &self,
        executable: &Path,
        args: &[String],
        environment: &[(String, String)],
        manager_name: &str,
        cancelled: &AtomicBool,
        on_log: &dyn Fn(String),
    ) -> ActionExecution;
}

struct SystemPackageActionRunner;

impl PackageActionRunner for SystemPackageActionRunner {
    fn run(
        &self,
        executable: &Path,
        args: &[String],
        environment: &[(String, String)],
        manager_name: &str,
        cancelled: &AtomicBool,
        on_log: &dyn Fn(String),
    ) -> ActionExecution {
        run_action_process(
            executable,
            args,
            environment,
            manager_name,
            cancelled,
            ACTION_TIMEOUT,
            on_log,
        )
    }
}

#[derive(Debug, Clone)]
struct ActivePackageAction {
    id: String,
    cancelled: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Default)]
pub struct ActionRegistry {
    plans: Arc<Mutex<HashMap<String, RegisteredPlan>>>,
    active: Arc<Mutex<Option<ActivePackageAction>>>,
}

impl ActionRegistry {
    pub fn register(&self, registered: RegisteredPlan) -> Result<(), AppError> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|error| AppError::Command(error.to_string()))?;
        plans.retain(|_, item| plan_is_fresh(&item.plan));
        if plans.len() >= 100 {
            return Err(AppError::Command(
                "待确认操作计划过多，请等待旧计划过期后重试".into(),
            ));
        }
        plans.insert(registered.plan.id.clone(), registered);
        Ok(())
    }

    pub fn take(&self, plan_id: &str) -> Result<RegisteredPlan, AppError> {
        let registered = self
            .plans
            .lock()
            .map_err(|error| AppError::Command(error.to_string()))?
            .remove(plan_id)
            .ok_or_else(|| AppError::Command("操作计划不存在、已执行或已过期".into()))?;
        if !plan_is_fresh(&registered.plan) {
            return Err(AppError::Command("操作计划已过期，请重新预检".into()));
        }
        Ok(registered)
    }

    pub fn begin(&self, action_id: &str) -> Result<Arc<AtomicBool>, AppError> {
        let mut active = self
            .active
            .lock()
            .map_err(|error| AppError::Command(error.to_string()))?;
        if active.is_some() {
            return Err(AppError::Command(
                "PACKAGE_ACTION_ALREADY_RUNNING：已有软件包操作正在进行".into(),
            ));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        *active = Some(ActivePackageAction {
            id: action_id.into(),
            cancelled: cancelled.clone(),
        });
        Ok(cancelled)
    }

    pub fn cancel(&self, action_id: &str) {
        if let Ok(active) = self.active.lock() {
            if let Some(active) = active.as_ref() {
                if active.id == action_id {
                    active.cancelled.store(true, Ordering::SeqCst);
                }
            }
        }
    }

    pub fn finish(&self, action_id: &str) {
        if let Ok(mut active) = self.active.lock() {
            if active.as_ref().is_some_and(|active| active.id == action_id) {
                *active = None;
            }
        }
    }
}

pub fn create_package_plan(
    storage: &Storage,
    registry: &ActionRegistry,
    manager_id: PackageManagerId,
    action: PackageAction,
    targets: Vec<String>,
) -> Result<PackageActionPlan, AppError> {
    let capability = package_action_capabilities(storage)?
        .into_iter()
        .find(|capability| capability.manager_id == manager_id && capability.action == action)
        .ok_or_else(|| AppError::Command("该操作没有可用能力声明".into()))?;
    if !capability.ready {
        let blocker = capability
            .checks
            .iter()
            .find(|check| check.status == crate::models::ActionCheckStatus::Blocked)
            .map(|check| format!("{:?}：{}", check.code, check.detail))
            .unwrap_or_else(|| "操作当前不可用".into());
        return Err(AppError::Command(blocker));
    }
    if !cfg!(target_os = "macos") {
        return Err(AppError::Command("软件包写操作当前仅支持 macOS".into()));
    }
    if !matches!(
        manager_id,
        PackageManagerId::Homebrew | PackageManagerId::Npm | PackageManagerId::Pnpm
    ) {
        return Err(AppError::Command("该包管理器暂不支持受控写操作".into()));
    }
    if storage.has_incomplete_action()? {
        return Err(AppError::Command(
            "PACKAGE_ACTION_RECOVERY_REQUIRED：上次操作可能中断，请先完成一次环境扫描".into(),
        ));
    }
    let snapshot = storage
        .latest_snapshot()?
        .ok_or_else(|| AppError::Command("请先完成一次环境扫描".into()))?;
    let manager = snapshot
        .managers
        .iter()
        .find(|manager| manager.id == manager_id)
        .ok_or_else(|| AppError::Command(format!("尚未发现 {}", manager_id.as_str())))?;
    if !matches!(manager.status, ManagerStatus::Available) {
        return Err(AppError::Command("包管理器可执行文件当前不可用".into()));
    }
    let trust_allowed = match manager_id {
        PackageManagerId::Homebrew => manager.execution_trust == ExecutionTrust::Managed,
        PackageManagerId::Npm | PackageManagerId::Pnpm => matches!(
            manager.execution_trust,
            ExecutionTrust::Managed | ExecutionTrust::UserManaged
        ),
        _ => false,
    };
    if !trust_allowed {
        return Err(AppError::Command(
            "包管理器不在允许写操作的可信执行目录中".into(),
        ));
    }
    let executable = manager
        .executable_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| AppError::Command("包管理器缺少可执行路径".into()))?;
    match manager_id {
        PackageManagerId::Homebrew => validate_homebrew_executable(&executable)?,
        PackageManagerId::Npm => {
            validate_npm_executable(&executable)?;
            validate_npm_active_node(&snapshot, &executable)?;
        }
        PackageManagerId::Pnpm => validate_pnpm_executable(&executable)?,
        _ => unreachable!(),
    }

    let installed = snapshot
        .packages
        .iter()
        .filter(|package| package.manager_id == manager_id)
        .map(|package| package.name.as_str())
        .collect::<Vec<_>>();
    let specification = match manager_id {
        PackageManagerId::Homebrew => build_homebrew_spec(action, targets, &installed)?,
        PackageManagerId::Npm => build_npm_spec(action, targets, &installed)?,
        PackageManagerId::Pnpm => build_pnpm_spec(action, targets, &installed)?,
        _ => unreachable!(),
    };
    let runner = CommandRunner::default();
    let mut warnings = specification.warnings;
    let mut preview_lines = Vec::new();
    let mut npm_context = None;

    match (manager_id, action) {
        (PackageManagerId::Homebrew, PackageAction::Install) => {
            let target = &specification.targets[0];
            preview_lines.push(format!(
                "Formula 名称已通过严格语法校验：{target}；存在性由 Homebrew 执行时验证。"
            ));
        }
        (PackageManagerId::Homebrew, PackageAction::Uninstall) => {
            let target = &specification.targets[0];
            let output = runner.run_cancellable(
                &executable,
                &["uses", "--installed", target],
                &AtomicBool::new(false),
            );
            if !output.success {
                return Err(AppError::Command(format!(
                    "无法检查 {target} 的被依赖关系：{}",
                    output.combined_output()
                )));
            }
            let dependents = parse_homebrew_dependents(&output.stdout);
            if !dependents.is_empty() {
                warnings.push(format!(
                    "{} 正被这些已安装 Formula 使用：{}。Homebrew 可能拒绝卸载。",
                    target,
                    dependents.join("、")
                ));
                preview_lines.push(format!("被依赖：{}", dependents.join("、")));
            } else {
                preview_lines.push("未发现依赖该 Formula 的已安装包。".into());
            }
        }
        (PackageManagerId::Homebrew, PackageAction::Cleanup) => {
            let output = runner.run_cancellable(
                &executable,
                &["cleanup", "--dry-run"],
                &AtomicBool::new(false),
            );
            if !output.success {
                return Err(AppError::Command(format!(
                    "Homebrew 缓存清理预览失败：{}",
                    output.combined_output()
                )));
            }
            preview_lines = preview_output_lines(&output.stdout, &output.stderr);
            if preview_lines.is_empty() {
                preview_lines.push("Homebrew 未报告可清理内容。".into());
            }
        }
        (PackageManagerId::Homebrew, PackageAction::Upgrade) => {
            preview_lines.push(format!(
                "将升级 {} 个已安装 Formula。",
                specification.targets.len()
            ));
        }
        (PackageManagerId::Npm, npm_action) => {
            let (node_path, prefix, cache) = npm_preflight(&runner, &executable)?;
            npm_context = Some(NpmExecutionContext {
                node_fingerprint: capture_executable_fingerprint(&node_path)?,
                prefix: prefix.clone(),
                cache: cache.clone(),
            });
            preview_lines.push(format!("关联 Node.js：{}", node_path.to_string_lossy()));
            preview_lines.push(format!("全局 prefix：{}", readable_action_path(&prefix)));
            preview_lines.push(format!("缓存目录：{}", readable_action_path(&cache)));
            match npm_action {
                PackageAction::Install => preview_lines.push(format!(
                    "包名已通过严格校验：{}；固定禁用 lifecycle scripts。",
                    specification.targets[0]
                )),
                PackageAction::Upgrade => preview_lines.push(format!(
                    "将升级 {} 个已扫描的 npm 全局包；固定禁用 lifecycle scripts。",
                    specification.targets.len()
                )),
                PackageAction::Uninstall => preview_lines.push(format!(
                    "将移除已扫描的 npm 全局包 {}；固定禁用 lifecycle scripts。",
                    specification.targets[0]
                )),
                PackageAction::Cleanup => preview_lines.push(
                    "仅执行 npm cache verify：校验缓存索引并回收无用内容，不强制清空缓存。".into(),
                ),
            }
            if path_looks_read_only(&prefix) {
                warnings.push(
                    "npm global prefix 当前显示为只读；操作可能失败，应用不会请求 sudo。".into(),
                );
            }
        }
        (PackageManagerId::Pnpm, PackageAction::Install) => preview_lines.push(format!(
            "包名已通过严格校验：{}；固定禁用 lifecycle scripts。",
            specification.targets[0]
        )),
        (PackageManagerId::Pnpm, PackageAction::Upgrade) => preview_lines.push(format!(
            "将升级 {} 个已扫描的 pnpm 全局包；固定禁用 lifecycle scripts。",
            specification.targets.len()
        )),
        (PackageManagerId::Pnpm, PackageAction::Uninstall) => preview_lines.push(format!(
            "将移除已扫描的 pnpm 全局包 {}；固定禁用 lifecycle scripts。",
            specification.targets[0]
        )),
        (PackageManagerId::Pnpm, PackageAction::Cleanup) => {
            let output =
                runner.run_cancellable(&executable, &["store", "path"], &AtomicBool::new(false));
            if !output.success {
                return Err(AppError::Command(format!(
                    "无法确认 pnpm store 路径：{}",
                    output.combined_output()
                )));
            }
            preview_lines.push(format!(
                "共享 store：{}",
                redact_and_truncate(output.stdout.trim())
            ));
            if let Some(size) = manager.cache_size_bytes {
                preview_lines.push(format!("当前扫描缓存大小：{} bytes", size));
            }
            warnings.push(
                "pnpm store prune 没有可靠的 dry-run；被清理内容可能需要后续重新下载。".into(),
            );
        }
        _ => unreachable!(),
    }

    let id = Uuid::new_v4().to_string();
    let command_preview = format!(
        "{} {}",
        executable.to_string_lossy(),
        specification.args.join(" ")
    );
    let plan = PackageActionPlan {
        id: id.clone(),
        manager_id,
        action,
        targets: specification.targets,
        command_preview,
        warnings,
        preview_lines,
        checks: capability.checks,
        requires_network: matches!(action, PackageAction::Install | PackageAction::Upgrade),
        created_at: Utc::now().to_rfc3339(),
    };
    registry.register(RegisteredPlan {
        plan: plan.clone(),
        executable_fingerprint: Some(capture_executable_fingerprint(&executable)?),
        npm_context,
        environment: action_environment(manager_id),
        executable,
        args: specification.args,
    })?;
    Ok(plan)
}

pub fn execute_registered_plan(
    registered: &RegisteredPlan,
    cancelled: &AtomicBool,
    on_log: &dyn Fn(String),
) -> ActionExecution {
    let validation = match registered.plan.manager_id {
        PackageManagerId::Homebrew => validate_homebrew_executable(&registered.executable),
        PackageManagerId::Npm => validate_npm_executable(&registered.executable),
        PackageManagerId::Pnpm => validate_pnpm_executable(&registered.executable),
        _ => Err(AppError::Command("该包管理器不支持受控写操作".into())),
    };
    if let Err(error) = validation {
        return ActionExecution {
            status: PackageActionStatus::Failed,
            logs: Vec::new(),
            error: Some(error.to_string()),
        };
    }
    if let Some(expected) = registered.executable_fingerprint.as_ref() {
        if let Err(error) = verify_executable_fingerprint(&registered.executable, expected) {
            return ActionExecution {
                status: PackageActionStatus::Failed,
                logs: Vec::new(),
                error: Some(error.to_string()),
            };
        }
    }
    if let Some(expected) = registered.npm_context.as_ref() {
        let runner = CommandRunner::default();
        let current =
            npm_preflight(&runner, &registered.executable).and_then(|(node, prefix, cache)| {
                Ok(NpmExecutionContext {
                    node_fingerprint: capture_executable_fingerprint(&node)?,
                    prefix,
                    cache,
                })
            });
        match current {
            Ok(current) if &current == expected => {}
            Ok(_) => {
                return ActionExecution {
                    status: PackageActionStatus::Failed,
                    logs: Vec::new(),
                    error: Some(
                        "npm 的 Node.js、global prefix 或 cache 在确认后发生变化，请重新生成计划"
                            .into(),
                    ),
                }
            }
            Err(error) => {
                return ActionExecution {
                    status: PackageActionStatus::Failed,
                    logs: Vec::new(),
                    error: Some(error.to_string()),
                }
            }
        }
    }
    execute_with_runner(registered, cancelled, on_log, &SystemPackageActionRunner)
}

fn execute_with_runner(
    registered: &RegisteredPlan,
    cancelled: &AtomicBool,
    on_log: &dyn Fn(String),
    runner: &dyn PackageActionRunner,
) -> ActionExecution {
    runner.run(
        &registered.executable,
        &registered.args,
        &registered.environment,
        registered.plan.manager_id.as_str(),
        cancelled,
        on_log,
    )
}

#[derive(Debug)]
struct ActionSpec {
    targets: Vec<String>,
    args: Vec<String>,
    warnings: Vec<String>,
}

fn build_homebrew_spec(
    action: PackageAction,
    targets: Vec<String>,
    installed: &[&str],
) -> Result<ActionSpec, AppError> {
    let expected = match action {
        PackageAction::Install | PackageAction::Uninstall => 1..=1,
        PackageAction::Upgrade => 1..=MAX_BATCH_TARGETS,
        PackageAction::Cleanup => 0..=0,
    };
    if !expected.contains(&targets.len()) {
        return Err(AppError::Command(match action {
            PackageAction::Install => "安装操作必须且只能指定一个 Formula".into(),
            PackageAction::Upgrade => {
                format!("升级操作必须指定 1 至 {MAX_BATCH_TARGETS} 个 Formula")
            }
            PackageAction::Uninstall => "卸载操作必须且只能指定一个 Formula".into(),
            PackageAction::Cleanup => "缓存清理操作不能指定 Formula".into(),
        }));
    }
    let mut normalized = Vec::new();
    for target in targets {
        let target = target.trim().to_string();
        if !is_valid_formula_name(&target) {
            return Err(AppError::Command(format!("Formula 名称不合法：{target}")));
        }
        if !normalized.contains(&target) {
            normalized.push(target);
        }
    }
    if action == PackageAction::Upgrade || action == PackageAction::Uninstall {
        if let Some(target) = normalized
            .iter()
            .find(|target| !installed.contains(&target.as_str()))
        {
            return Err(AppError::Command(format!(
                "只能{}扫描结果中的已安装 Formula：{target}",
                if action == PackageAction::Upgrade {
                    "升级"
                } else {
                    "卸载"
                }
            )));
        }
    }
    let mut warnings = vec![
        "该操作会修改本机 Homebrew 环境，无法保证自动回滚。".into(),
        "操作期间请勿退出应用；若应用异常退出，请重新启动并刷新扫描确认实际状态。".into(),
    ];
    if action == PackageAction::Install && installed.contains(&normalized[0].as_str()) {
        warnings.push("目标 Formula 已安装，Homebrew 可能不会产生变化。".into());
    }
    if action == PackageAction::Cleanup {
        warnings.push("缓存清理会删除 Homebrew 判定为可安全移除的旧下载和版本。".into());
    }
    let command = match action {
        PackageAction::Install => "install",
        PackageAction::Upgrade => "upgrade",
        PackageAction::Uninstall => "uninstall",
        PackageAction::Cleanup => "cleanup",
    };
    let mut args = vec![command.into()];
    args.extend(normalized.iter().cloned());
    Ok(ActionSpec {
        targets: normalized,
        args,
        warnings,
    })
}

fn build_pnpm_spec(
    action: PackageAction,
    targets: Vec<String>,
    installed: &[&str],
) -> Result<ActionSpec, AppError> {
    let expected = match action {
        PackageAction::Install | PackageAction::Uninstall => 1..=1,
        PackageAction::Upgrade => 1..=MAX_BATCH_TARGETS,
        PackageAction::Cleanup => 0..=0,
    };
    if !expected.contains(&targets.len()) {
        return Err(AppError::Command(match action {
            PackageAction::Install => "安装操作必须且只能指定一个 pnpm 包".into(),
            PackageAction::Upgrade => {
                format!("升级操作必须指定 1 至 {MAX_BATCH_TARGETS} 个 pnpm 全局包")
            }
            PackageAction::Uninstall => "卸载操作必须且只能指定一个 pnpm 全局包".into(),
            PackageAction::Cleanup => "pnpm store 清理不能指定软件包".into(),
        }));
    }

    let mut normalized = Vec::new();
    for target in targets {
        let target = target.trim().to_string();
        if !is_valid_registry_package_name(&target) {
            return Err(AppError::Command(format!("pnpm 包名不合法：{target}")));
        }
        if !normalized.contains(&target) {
            normalized.push(target);
        }
    }
    if matches!(action, PackageAction::Upgrade | PackageAction::Uninstall) {
        if let Some(target) = normalized
            .iter()
            .find(|target| !installed.contains(&target.as_str()))
        {
            return Err(AppError::Command(format!(
                "只能{}扫描结果中的 pnpm 全局包：{target}",
                if action == PackageAction::Upgrade {
                    "升级"
                } else {
                    "卸载"
                }
            )));
        }
    }
    if action != PackageAction::Cleanup && normalized.iter().any(|target| target == "pnpm") {
        return Err(AppError::Command(
            "不允许通过 pnpm 写操作修改 pnpm 自身；请使用独立运行时管理流程".into(),
        ));
    }

    let mut warnings = vec![
        "该操作会修改本机 pnpm 全局环境，无法保证自动回滚。".into(),
        "执行固定使用 --ignore-scripts，不运行软件包 lifecycle scripts。".into(),
        "操作期间请勿退出应用；异常退出后必须先重新扫描。".into(),
    ];
    if action == PackageAction::Install && installed.contains(&normalized[0].as_str()) {
        warnings.push("目标包已经存在，pnpm 可能不会产生变化。".into());
    }
    if action == PackageAction::Cleanup {
        warnings.push("pnpm store 可能被多个项目共享；清理后部分内容需要重新下载。".into());
    }

    let args = match action {
        PackageAction::Install => vec![
            "add".into(),
            "--global".into(),
            "--ignore-scripts".into(),
            normalized[0].clone(),
        ],
        PackageAction::Upgrade => {
            let mut args = vec![
                "update".into(),
                "--global".into(),
                "--ignore-scripts".into(),
            ];
            args.extend(normalized.iter().cloned());
            args
        }
        PackageAction::Uninstall => vec![
            "remove".into(),
            "--global".into(),
            "--ignore-scripts".into(),
            normalized[0].clone(),
        ],
        PackageAction::Cleanup => vec!["store".into(), "prune".into()],
    };
    Ok(ActionSpec {
        targets: normalized,
        args,
        warnings,
    })
}

fn build_npm_spec(
    action: PackageAction,
    targets: Vec<String>,
    installed: &[&str],
) -> Result<ActionSpec, AppError> {
    let expected = match action {
        PackageAction::Install | PackageAction::Uninstall => 1..=1,
        PackageAction::Upgrade => 1..=MAX_BATCH_TARGETS,
        PackageAction::Cleanup => 0..=0,
    };
    if !expected.contains(&targets.len()) {
        return Err(AppError::Command(match action {
            PackageAction::Install => "安装操作必须且只能指定一个 npm 包".into(),
            PackageAction::Upgrade => {
                format!("升级操作必须指定 1 至 {MAX_BATCH_TARGETS} 个 npm 全局包")
            }
            PackageAction::Uninstall => "卸载操作必须且只能指定一个 npm 全局包".into(),
            PackageAction::Cleanup => "npm 缓存校验不能指定软件包".into(),
        }));
    }
    let mut normalized = Vec::new();
    for target in targets {
        let target = target.trim().to_string();
        if !is_valid_registry_package_name(&target) {
            return Err(AppError::Command(format!("npm 包名不合法：{target}")));
        }
        if !normalized.contains(&target) {
            normalized.push(target);
        }
    }
    if matches!(action, PackageAction::Upgrade | PackageAction::Uninstall) {
        if let Some(target) = normalized
            .iter()
            .find(|target| !installed.contains(&target.as_str()))
        {
            return Err(AppError::Command(format!(
                "只能{}扫描结果中的 npm 全局包：{target}",
                if action == PackageAction::Upgrade {
                    "升级"
                } else {
                    "卸载"
                }
            )));
        }
    }
    if action != PackageAction::Cleanup && normalized.iter().any(|target| target == "npm") {
        return Err(AppError::Command(
            "不允许通过 npm 写操作修改 npm 自身；请使用独立 Node.js 运行时管理流程".into(),
        ));
    }
    let mut warnings = vec![
        "该操作会修改本机 npm 全局环境，无法保证自动回滚。".into(),
        "执行固定使用 --ignore-scripts，不运行软件包 lifecycle scripts。".into(),
        "部分 CLI 依赖安装脚本，禁用脚本后功能可能不完整。".into(),
        "操作期间请勿退出应用；异常退出后必须先重新扫描。".into(),
    ];
    if action == PackageAction::Install && installed.contains(&normalized[0].as_str()) {
        warnings.push("目标包已经存在，npm 可能不会产生变化。".into());
    }
    if action == PackageAction::Cleanup {
        warnings
            .push("npm cache verify 会校验缓存索引并回收无用内容，但不会执行强制完整清空。".into());
    }
    let args = match action {
        PackageAction::Install => vec![
            "install".into(),
            "--global".into(),
            "--ignore-scripts".into(),
            normalized[0].clone(),
        ],
        PackageAction::Upgrade => {
            let mut args = vec![
                "update".into(),
                "--global".into(),
                "--ignore-scripts".into(),
            ];
            args.extend(normalized.iter().cloned());
            args
        }
        PackageAction::Uninstall => vec![
            "uninstall".into(),
            "--global".into(),
            "--ignore-scripts".into(),
            normalized[0].clone(),
        ],
        PackageAction::Cleanup => vec!["cache".into(), "verify".into()],
    };
    Ok(ActionSpec {
        targets: normalized,
        args,
        warnings,
    })
}

pub(super) fn validate_homebrew_executable(executable: &Path) -> Result<(), AppError> {
    let canonical = executable
        .canonicalize()
        .map_err(|error| AppError::Command(format!("无法验证 Homebrew 路径：{error}")))?;
    let allowed = [
        Path::new("/opt/homebrew/bin/brew"),
        Path::new("/usr/local/bin/brew"),
    ];
    if allowed
        .iter()
        .any(|path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf()) == canonical)
    {
        Ok(())
    } else {
        Err(AppError::Command(
            "Homebrew 写操作仅允许受信任的标准安装路径".into(),
        ))
    }
}

fn action_environment(manager_id: PackageManagerId) -> Vec<(String, String)> {
    if manager_id == PackageManagerId::Homebrew {
        vec![
            ("HOMEBREW_NO_AUTO_UPDATE".into(), "1".into()),
            ("HOMEBREW_NO_ANALYTICS".into(), "1".into()),
            ("HOMEBREW_NO_ENV_HINTS".into(), "1".into()),
        ]
    } else {
        Vec::new()
    }
}

fn capture_executable_fingerprint(path: &Path) -> Result<ExecutableFingerprint, AppError> {
    let canonical_path = path
        .canonicalize()
        .map_err(|error| AppError::Command(format!("无法验证包管理器路径：{error}")))?;
    let metadata = canonical_path
        .metadata()
        .map_err(|error| AppError::Command(format!("无法读取包管理器文件信息：{error}")))?;
    if !metadata.is_file() {
        return Err(AppError::Command("包管理器路径不是普通文件".into()));
    }
    Ok(ExecutableFingerprint {
        canonical_path,
        size: metadata.len(),
        modified_at: metadata.modified().ok(),
    })
}

fn verify_executable_fingerprint(
    path: &Path,
    expected: &ExecutableFingerprint,
) -> Result<(), AppError> {
    if &capture_executable_fingerprint(path)? == expected {
        Ok(())
    } else {
        Err(AppError::Command(
            "包管理器可执行文件在计划确认后发生变化，请重新生成计划".into(),
        ))
    }
}

pub(super) fn validate_pnpm_executable(executable: &Path) -> Result<(), AppError> {
    if executable.file_name().and_then(|name| name.to_str()) != Some("pnpm") {
        return Err(AppError::Command(
            "pnpm 写操作仅允许扫描得到的 pnpm 可执行文件".into(),
        ));
    }
    let canonical = executable
        .canonicalize()
        .map_err(|error| AppError::Command(format!("无法验证 pnpm 路径：{error}")))?;
    if !canonical.is_file()
        || !matches!(
            execution_trust(&canonical, &[]),
            ExecutionTrust::System | ExecutionTrust::Managed | ExecutionTrust::UserManaged
        )
    {
        return Err(AppError::Command(
            "pnpm 可执行文件不在允许写操作的可信目录中".into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_npm_executable(executable: &Path) -> Result<(), AppError> {
    if executable.file_name().and_then(|name| name.to_str()) != Some("npm") {
        return Err(AppError::Command(
            "npm 写操作仅允许扫描得到的 npm 可执行文件".into(),
        ));
    }
    let canonical = executable
        .canonicalize()
        .map_err(|error| AppError::Command(format!("无法验证 npm 路径：{error}")))?;
    let npm_trust = execution_trust(&canonical, &[]);
    if !canonical.is_file()
        || !matches!(
            npm_trust,
            ExecutionTrust::Managed | ExecutionTrust::UserManaged
        )
    {
        return Err(AppError::Command(
            "npm 可执行文件不在允许写操作的可信目录中".into(),
        ));
    }
    let node = executable
        .parent()
        .map(|directory| directory.join("node"))
        .ok_or_else(|| AppError::Command("无法定位 npm 关联的 Node.js".into()))?;
    let node_canonical = node.canonicalize().map_err(|_| {
        AppError::Command("npm 同目录缺少 Node.js；为避免 PATH 运行时错配，已拒绝写操作".into())
    })?;
    if !node_canonical.is_file() || execution_trust(&node_canonical, &[]) != npm_trust {
        return Err(AppError::Command(
            "npm 与同目录 Node.js 的信任来源不一致，已拒绝写操作".into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_npm_active_node(
    snapshot: &crate::models::EnvironmentScan,
    executable: &Path,
) -> Result<(), AppError> {
    let sibling = executable
        .parent()
        .map(|directory| directory.join("node"))
        .and_then(|path| path.canonicalize().ok())
        .ok_or_else(|| AppError::Command("无法验证 npm 同目录 Node.js".into()))?;
    let active = snapshot
        .path_observations
        .iter()
        .find(|observation| observation.command == "node")
        .and_then(|observation| observation.active_path.as_deref())
        .map(PathBuf::from)
        .and_then(|path| path.canonicalize().ok())
        .ok_or_else(|| AppError::Command("扫描快照缺少可验证的 PATH Node.js".into()))?;
    if active != sibling {
        return Err(AppError::Command(format!(
            "npm 关联 Node.js 与 PATH 当前 Node.js 不一致：{}；请先解决运行时冲突",
            readable_action_path(&active)
        )));
    }
    Ok(())
}

pub(super) fn npm_preflight(
    runner: &CommandRunner,
    executable: &Path,
) -> Result<(PathBuf, PathBuf, PathBuf), AppError> {
    let node = executable
        .parent()
        .map(|directory| directory.join("node"))
        .ok_or_else(|| AppError::Command("无法定位 npm 关联的 Node.js".into()))?;
    let prefix = npm_config_path(runner, executable, "prefix")?;
    let cache = npm_config_path(runner, executable, "cache")?;
    Ok((node, prefix, cache))
}

fn npm_config_path(
    runner: &CommandRunner,
    executable: &Path,
    key: &str,
) -> Result<PathBuf, AppError> {
    let output =
        runner.run_cancellable(executable, &["config", "get", key], &AtomicBool::new(false));
    if !output.success {
        return Err(AppError::Command(format!(
            "无法读取 npm {key}：{}",
            output.combined_output()
        )));
    }
    let value = output.stdout.trim();
    if value.is_empty() || value.lines().count() != 1 {
        return Err(AppError::Command(format!("npm {key} 返回了无效路径")));
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() || !is_allowed_npm_data_path(&path) {
        return Err(AppError::Command(format!(
            "npm {key} 不在允许的用户或受管理目录中：{}",
            redact_and_truncate(value)
        )));
    }
    Ok(path)
}

fn is_allowed_npm_data_path(path: &Path) -> bool {
    path.starts_with("/opt/homebrew")
        || path.starts_with("/usr/local")
        || dirs::home_dir().is_some_and(|home| path.starts_with(home))
}

pub(super) fn path_looks_read_only(path: &Path) -> bool {
    path.metadata()
        .ok()
        .or_else(|| path.parent().and_then(|parent| parent.metadata().ok()))
        .map(|metadata| metadata.permissions().readonly())
        .unwrap_or(false)
}

fn readable_action_path(path: &Path) -> String {
    let value = path.to_string_lossy().into_owned();
    if let Some(home) = dirs::home_dir() {
        if let Ok(relative) = path.strip_prefix(home) {
            return format!("~/{}", relative.to_string_lossy());
        }
    }
    value
}

pub(crate) fn is_valid_formula_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && !value.contains("..")
        && !value.contains("//")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/@+_.-".contains(&byte))
}

pub(crate) fn is_valid_registry_package_name(value: &str) -> bool {
    if value.is_empty() || value.len() > 214 || value.starts_with('.') || value.contains("..") {
        return false;
    }
    let valid_segment = |segment: &str| {
        !segment.is_empty()
            && segment.len() <= 128
            && segment
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    };
    if let Some(scoped) = value.strip_prefix('@') {
        let mut parts = scoped.split('/');
        matches!((parts.next(), parts.next(), parts.next()), (Some(scope), Some(name), None) if valid_segment(scope) && valid_segment(name))
    } else {
        !value.contains('/') && !value.contains('@') && valid_segment(value)
    }
}

fn plan_is_fresh(plan: &PackageActionPlan) -> bool {
    chrono::DateTime::parse_from_rfc3339(&plan.created_at)
        .ok()
        .is_some_and(|created| {
            Utc::now().signed_duration_since(created) <= chrono::Duration::seconds(PLAN_TTL_SECONDS)
        })
}

fn preview_output_lines(stdout: &str, stderr: &str) -> Vec<String> {
    [stdout, stderr]
        .into_iter()
        .flat_map(str::lines)
        .map(redact_and_truncate)
        .filter(|line| !line.is_empty())
        .take(20)
        .collect()
}

fn parse_homebrew_dependents(output: &str) -> Vec<String> {
    output
        .split_whitespace()
        .filter(|value| is_valid_formula_name(value))
        .map(str::to_string)
        .collect()
}

fn run_action_process(
    executable: &Path,
    args: &[String],
    environment: &[(String, String)],
    manager_name: &str,
    cancelled: &AtomicBool,
    timeout: Duration,
    on_log: &dyn Fn(String),
) -> ActionExecution {
    let mut child = match Command::new(executable)
        .args(args)
        .env("NO_COLOR", "1")
        .envs(environment.iter().cloned())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return ActionExecution {
                status: PackageActionStatus::Failed,
                logs: Vec::new(),
                error: Some(error.to_string()),
            }
        }
    };
    let (sender, receiver) = mpsc::channel::<String>();
    let stdout_reader = child
        .stdout
        .take()
        .map(|reader| stream_lines(reader, sender.clone()));
    let stderr_reader = child
        .stderr
        .take()
        .map(|reader| stream_lines(reader, sender));
    let started = Instant::now();
    let mut logs = Vec::new();
    let (status, error) = loop {
        drain_progress(&receiver, &mut logs, on_log);
        if cancelled.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            break (
                PackageActionStatus::Unknown,
                Some("操作已终止；包管理器状态未知，已强制重新扫描。".into()),
            );
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            break (
                PackageActionStatus::Unknown,
                Some("操作超过 30 分钟并已终止；包管理器状态未知。".into()),
            );
        }
        match child.try_wait() {
            Ok(Some(exit)) if exit.success() => break (PackageActionStatus::Succeeded, None),
            Ok(Some(exit)) => {
                break (
                    PackageActionStatus::Failed,
                    Some(format!(
                        "{manager_name} 退出码：{}",
                        exit.code().unwrap_or(-1)
                    )),
                )
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break (PackageActionStatus::Unknown, Some(error.to_string()));
            }
        }
    };
    let _ = stdout_reader.map(thread::JoinHandle::join);
    let _ = stderr_reader.map(thread::JoinHandle::join);
    drain_progress(&receiver, &mut logs, on_log);
    ActionExecution {
        status,
        logs,
        error,
    }
}

fn stream_lines<R: Read + Send + 'static>(
    reader: R,
    sender: mpsc::Sender<String>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            let line = redact_and_truncate(&line);
            if !line.is_empty() && sender.send(line).is_err() {
                break;
            }
        }
    })
}

fn drain_progress(
    receiver: &mpsc::Receiver<String>,
    logs: &mut Vec<String>,
    on_log: &dyn Fn(String),
) {
    while let Ok(line) = receiver.try_recv() {
        on_log(line.clone());
        let stored_bytes = logs.iter().map(String::len).sum::<usize>();
        if logs.len() < MAX_PROGRESS_LINES
            && stored_bytes.saturating_add(line.len()) <= MAX_ACTION_LOG_BYTES
        {
            logs.push(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeRunner {
        status: PackageActionStatus,
        calls: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl PackageActionRunner for FakeRunner {
        fn run(
            &self,
            _executable: &Path,
            args: &[String],
            _environment: &[(String, String)],
            manager_name: &str,
            cancelled: &AtomicBool,
            on_log: &dyn Fn(String),
        ) -> ActionExecution {
            self.calls
                .lock()
                .unwrap()
                .push((manager_name.into(), args.to_vec()));
            if cancelled.load(Ordering::SeqCst) {
                return ActionExecution {
                    status: PackageActionStatus::Unknown,
                    logs: vec![],
                    error: Some("模拟取消；状态未知".into()),
                };
            }
            on_log("模拟执行完成".into());
            ActionExecution {
                status: self.status,
                logs: vec!["模拟执行完成".into()],
                error: (self.status == PackageActionStatus::Failed).then(|| "模拟失败".into()),
            }
        }
    }

    #[test]
    fn builds_only_explicit_homebrew_command_shapes() {
        let installed = ["git", "ripgrep", "node@22"];
        let install =
            build_homebrew_spec(PackageAction::Install, vec!["jq".into()], &installed).unwrap();
        assert_eq!(install.args, vec!["install", "jq"]);
        let upgrade = build_homebrew_spec(
            PackageAction::Upgrade,
            vec!["git".into(), "ripgrep".into()],
            &installed,
        )
        .unwrap();
        assert_eq!(upgrade.args, vec!["upgrade", "git", "ripgrep"]);
        let uninstall =
            build_homebrew_spec(PackageAction::Uninstall, vec!["node@22".into()], &installed)
                .unwrap();
        assert_eq!(uninstall.args, vec!["uninstall", "node@22"]);
        let cleanup = build_homebrew_spec(PackageAction::Cleanup, vec![], &installed).unwrap();
        assert_eq!(cleanup.args, vec!["cleanup"]);
    }

    #[test]
    fn builds_only_explicit_pnpm_global_command_shapes() {
        let installed = ["typescript", "@scope/tool"];
        let install =
            build_pnpm_spec(PackageAction::Install, vec!["eslint".into()], &installed).unwrap();
        assert_eq!(
            install.args,
            vec!["add", "--global", "--ignore-scripts", "eslint"]
        );
        let upgrade = build_pnpm_spec(
            PackageAction::Upgrade,
            vec!["typescript".into(), "@scope/tool".into()],
            &installed,
        )
        .unwrap();
        assert_eq!(
            upgrade.args,
            vec![
                "update",
                "--global",
                "--ignore-scripts",
                "typescript",
                "@scope/tool"
            ]
        );
        let uninstall = build_pnpm_spec(
            PackageAction::Uninstall,
            vec!["@scope/tool".into()],
            &installed,
        )
        .unwrap();
        assert_eq!(
            uninstall.args,
            vec!["remove", "--global", "--ignore-scripts", "@scope/tool"]
        );
        let cleanup = build_pnpm_spec(PackageAction::Cleanup, vec![], &installed).unwrap();
        assert_eq!(cleanup.args, vec!["store", "prune"]);
    }

    #[test]
    fn builds_only_explicit_npm_global_command_shapes() {
        let installed = ["typescript", "@scope/tool"];
        let install =
            build_npm_spec(PackageAction::Install, vec!["eslint".into()], &installed).unwrap();
        assert_eq!(
            install.args,
            vec!["install", "--global", "--ignore-scripts", "eslint"]
        );
        let upgrade = build_npm_spec(
            PackageAction::Upgrade,
            vec!["typescript".into(), "@scope/tool".into()],
            &installed,
        )
        .unwrap();
        assert_eq!(
            upgrade.args,
            vec![
                "update",
                "--global",
                "--ignore-scripts",
                "typescript",
                "@scope/tool"
            ]
        );
        let uninstall = build_npm_spec(
            PackageAction::Uninstall,
            vec!["@scope/tool".into()],
            &installed,
        )
        .unwrap();
        assert_eq!(
            uninstall.args,
            vec!["uninstall", "--global", "--ignore-scripts", "@scope/tool"]
        );
        let cleanup = build_npm_spec(PackageAction::Cleanup, vec![], &installed).unwrap();
        assert_eq!(cleanup.args, vec!["cache", "verify"]);
    }

    #[test]
    fn rejects_npm_specs_urls_paths_flags_and_unscanned_mutations() {
        for target in [
            "--force",
            "pkg@next",
            "@scope/pkg@1",
            "https://example.com/pkg.tgz",
            "file:../pkg",
            "workspace:*",
            "github:user/repo",
            "../pkg",
        ] {
            assert!(build_npm_spec(PackageAction::Install, vec![target.into()], &[]).is_err());
        }
        assert!(build_npm_spec(
            PackageAction::Upgrade,
            vec!["unknown".into()],
            &["typescript"]
        )
        .is_err());
        assert!(build_npm_spec(PackageAction::Install, vec!["npm".into()], &[]).is_err());
    }

    #[test]
    fn isolates_manager_environment_and_detects_executable_changes() {
        assert!(action_environment(PackageManagerId::Npm).is_empty());
        assert!(action_environment(PackageManagerId::Pnpm).is_empty());
        assert!(action_environment(PackageManagerId::Homebrew)
            .iter()
            .any(|(key, _)| key == "HOMEBREW_NO_AUTO_UPDATE"));

        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("tool");
        std::fs::write(&executable, "first").unwrap();
        let before = capture_executable_fingerprint(&executable).unwrap();
        std::fs::write(&executable, "second-longer").unwrap();
        let after = capture_executable_fingerprint(&executable).unwrap();
        assert_ne!(before, after);
        assert!(verify_executable_fingerprint(&executable, &before).is_err());
    }

    #[test]
    fn restricts_npm_data_paths_to_user_and_managed_roots() {
        assert!(is_allowed_npm_data_path(Path::new("/opt/homebrew/lib")));
        assert!(is_allowed_npm_data_path(Path::new(
            "/usr/local/lib/node_modules"
        )));
        assert!(!is_allowed_npm_data_path(Path::new(
            "/usr/lib/node_modules"
        )));
        assert!(!is_allowed_npm_data_path(Path::new("/tmp/npm-prefix")));
    }

    #[test]
    fn rejects_npm_when_path_node_differs_from_its_sibling_runtime() {
        let directory = tempfile::tempdir().unwrap();
        let npm_directory = directory.path().join("npm-bin");
        let other_directory = directory.path().join("other-bin");
        std::fs::create_dir_all(&npm_directory).unwrap();
        std::fs::create_dir_all(&other_directory).unwrap();
        let npm = npm_directory.join("npm");
        let sibling_node = npm_directory.join("node");
        let other_node = other_directory.join("node");
        for path in [&npm, &sibling_node, &other_node] {
            std::fs::write(path, "fixture").unwrap();
        }
        let scan = |active: &Path| {
            serde_json::from_value::<crate::models::EnvironmentScan>(serde_json::json!({
                "managers": [], "packages": [], "projects": [], "scanRoots": [],
                "healthIssues": [], "logs": [], "pathObservations": [{
                    "command": "node", "activePath": active, "alternatives": [], "hasConflict": false
                }],
                "scannedAt": "2026-07-11T00:00:00Z", "partialFailures": 0
            }))
            .unwrap()
        };
        assert!(validate_npm_active_node(&scan(&sibling_node), &npm).is_ok());
        assert!(validate_npm_active_node(&scan(&other_node), &npm).is_err());
    }

    #[test]
    fn generic_action_engine_accepts_a_fake_runner_without_spawning_package_managers() {
        let spec = build_pnpm_spec(PackageAction::Install, vec!["eslint".into()], &[]).unwrap();
        let registered = RegisteredPlan {
            plan: PackageActionPlan {
                id: "plan-fake".into(),
                manager_id: PackageManagerId::Pnpm,
                action: PackageAction::Install,
                targets: spec.targets,
                command_preview: "pnpm add --global --ignore-scripts eslint".into(),
                warnings: spec.warnings,
                preview_lines: vec![],
                checks: vec![],
                requires_network: true,
                created_at: Utc::now().to_rfc3339(),
            },
            executable: "/not/executed/pnpm".into(),
            args: spec.args,
            environment: vec![],
            executable_fingerprint: None,
            npm_context: None,
        };
        let runner = FakeRunner {
            status: PackageActionStatus::Succeeded,
            calls: Mutex::new(vec![]),
        };
        let progress = Mutex::new(vec![]);
        let result = execute_with_runner(
            &registered,
            &AtomicBool::new(false),
            &|line| progress.lock().unwrap().push(line),
            &runner,
        );
        assert_eq!(result.status, PackageActionStatus::Succeeded);
        assert_eq!(*progress.lock().unwrap(), vec!["模拟执行完成"]);
        assert_eq!(
            *runner.calls.lock().unwrap(),
            vec![(
                "pnpm".into(),
                vec![
                    "add".into(),
                    "--global".into(),
                    "--ignore-scripts".into(),
                    "eslint".into()
                ]
            )]
        );
    }

    #[test]
    fn rejects_pnpm_specs_urls_paths_flags_and_unscanned_mutations() {
        for target in [
            "--force",
            "pkg@next",
            "@scope/pkg@1",
            "https://example.com/pkg.tgz",
            "file:../pkg",
            "github:user/repo",
            "../pkg",
            "@scope",
            "@scope/",
        ] {
            assert!(build_pnpm_spec(PackageAction::Install, vec![target.into()], &[]).is_err());
        }
        assert!(build_pnpm_spec(
            PackageAction::Uninstall,
            vec!["unknown".into()],
            &["typescript"]
        )
        .is_err());
        assert!(build_pnpm_spec(PackageAction::Install, vec!["pnpm".into()], &[]).is_err());
    }

    #[test]
    fn rejects_argument_injection_uninstalled_targets_and_oversized_batches() {
        for target in [
            "--force", "@scope", "/git", "git;rm", "$(touch)", "../git", "tap//git", "git name",
        ] {
            assert!(build_homebrew_spec(PackageAction::Install, vec![target.into()], &[]).is_err());
        }
        assert!(
            build_homebrew_spec(PackageAction::Upgrade, vec!["unknown".into()], &["git"]).is_err()
        );
        assert!(build_homebrew_spec(
            PackageAction::Upgrade,
            (0..=MAX_BATCH_TARGETS)
                .map(|index| format!("pkg-{index}"))
                .collect(),
            &[]
        )
        .is_err());
    }

    #[test]
    fn parses_only_safe_installed_dependents_and_limits_cleanup_preview() {
        assert_eq!(
            parse_homebrew_dependents("node ffmpeg\n--flag bad;name"),
            vec!["node", "ffmpeg"]
        );
        let preview = preview_output_lines(
            &(0..25)
                .map(|index| format!("Would remove: cache-{index}"))
                .collect::<Vec<_>>()
                .join("\n"),
            "",
        );
        assert_eq!(preview.len(), 20);
    }

    #[test]
    fn registry_consumes_plans_once_and_cancels_only_the_active_action() {
        let registry = ActionRegistry::default();
        let plan = PackageActionPlan {
            id: "plan-1".into(),
            manager_id: PackageManagerId::Homebrew,
            action: PackageAction::Cleanup,
            targets: vec![],
            command_preview: "brew cleanup".into(),
            warnings: vec![],
            preview_lines: vec![],
            checks: vec![],
            requires_network: false,
            created_at: Utc::now().to_rfc3339(),
        };
        registry
            .register(RegisteredPlan {
                plan,
                executable: "/bin/echo".into(),
                args: vec!["cleanup".into()],
                environment: vec![],
                executable_fingerprint: None,
                npm_context: None,
            })
            .unwrap();
        assert!(registry.take("plan-1").is_ok());
        assert!(registry.take("plan-1").is_err());
        let cancelled = registry.begin("action-1").unwrap();
        registry.cancel("other");
        assert!(!cancelled.load(Ordering::SeqCst));
        registry.cancel("action-1");
        assert!(cancelled.load(Ordering::SeqCst));
        registry.finish("action-1");
    }

    #[test]
    fn registry_rejects_expired_plans() {
        let registry = ActionRegistry::default();
        let plan = PackageActionPlan {
            id: "expired".into(),
            manager_id: PackageManagerId::Homebrew,
            action: PackageAction::Cleanup,
            targets: vec![],
            command_preview: "brew cleanup".into(),
            warnings: vec![],
            preview_lines: vec![],
            checks: vec![],
            requires_network: false,
            created_at: (Utc::now() - chrono::Duration::minutes(11)).to_rfc3339(),
        };
        registry
            .register(RegisteredPlan {
                plan,
                executable: "/bin/echo".into(),
                args: vec!["cleanup".into()],
                environment: vec![],
                executable_fingerprint: None,
                npm_context: None,
            })
            .unwrap();
        assert!(registry.take("expired").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn process_runner_streams_output_and_reports_failures() {
        let streamed = Mutex::new(Vec::new());
        let success = run_action_process(
            Path::new("/bin/echo"),
            &["installed jq".into()],
            &[],
            "test-manager",
            &AtomicBool::new(false),
            Duration::from_secs(2),
            &|line| streamed.lock().unwrap().push(line),
        );
        assert_eq!(success.status, PackageActionStatus::Succeeded);
        assert_eq!(success.logs, vec!["installed jq"]);
        assert_eq!(*streamed.lock().unwrap(), vec!["installed jq"]);

        let failed = run_action_process(
            Path::new("/usr/bin/false"),
            &[],
            &[],
            "test-manager",
            &AtomicBool::new(false),
            Duration::from_secs(2),
            &|_| {},
        );
        assert_eq!(failed.status, PackageActionStatus::Failed);
        assert!(failed.error.unwrap().contains("退出码"));
    }

    #[cfg(unix)]
    #[test]
    fn timeout_marks_process_result_unknown() {
        let result = run_action_process(
            Path::new("/bin/sleep"),
            &["2".into()],
            &[],
            "test-manager",
            &AtomicBool::new(false),
            Duration::from_millis(30),
            &|_| {},
        );
        assert_eq!(result.status, PackageActionStatus::Unknown);
        assert!(result.error.unwrap().contains("状态未知"));
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_marks_process_result_unknown() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = cancelled.clone();
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(60));
            signal.store(true, Ordering::SeqCst);
        });
        let result = run_action_process(
            Path::new("/bin/sleep"),
            &["2".into()],
            &[],
            "test-manager",
            &cancelled,
            Duration::from_secs(2),
            &|_| {},
        );
        canceller.join().unwrap();
        assert_eq!(result.status, PackageActionStatus::Unknown);
        assert!(result.error.unwrap().contains("状态未知"));
    }
}
