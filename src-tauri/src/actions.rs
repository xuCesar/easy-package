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
    time::{Duration, Instant},
};

use chrono::Utc;
use uuid::Uuid;

use crate::{
    adapters::runner::{redact_and_truncate, CommandRunner},
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
}

#[derive(Debug, Clone)]
pub struct ActionExecution {
    pub status: PackageActionStatus,
    pub logs: Vec<String>,
    pub error: Option<String>,
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

pub fn create_homebrew_plan(
    storage: &Storage,
    registry: &ActionRegistry,
    action: PackageAction,
    targets: Vec<String>,
) -> Result<PackageActionPlan, AppError> {
    if !cfg!(target_os = "macos") {
        return Err(AppError::Command("Homebrew 写操作当前仅支持 macOS".into()));
    }
    let snapshot = storage
        .latest_snapshot()?
        .ok_or_else(|| AppError::Command("请先完成一次环境扫描".into()))?;
    let manager = snapshot
        .managers
        .iter()
        .find(|manager| manager.id == PackageManagerId::Homebrew)
        .ok_or_else(|| AppError::Command("尚未发现 Homebrew".into()))?;
    if !matches!(manager.status, ManagerStatus::Available)
        || manager.execution_trust != ExecutionTrust::Managed
    {
        return Err(AppError::Command(
            "Homebrew 可执行文件不可用或不在受信任的管理器目录中".into(),
        ));
    }
    let executable = manager
        .executable_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| AppError::Command("Homebrew 缺少可执行路径".into()))?;
    validate_homebrew_executable(&executable)?;

    let installed = snapshot
        .packages
        .iter()
        .filter(|package| package.manager_id == PackageManagerId::Homebrew)
        .map(|package| package.name.as_str())
        .collect::<Vec<_>>();
    let specification = build_homebrew_spec(action, targets, &installed)?;
    let runner = CommandRunner::default();
    let mut warnings = specification.warnings;
    let mut preview_lines = Vec::new();

    match action {
        PackageAction::Install => {
            let target = &specification.targets[0];
            preview_lines.push(format!(
                "Formula 名称已通过严格语法校验：{target}；存在性由 Homebrew 执行时验证。"
            ));
        }
        PackageAction::Uninstall => {
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
        PackageAction::Cleanup => {
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
        PackageAction::Upgrade => {
            preview_lines.push(format!(
                "将升级 {} 个已安装 Formula。",
                specification.targets.len()
            ));
        }
    }

    let id = Uuid::new_v4().to_string();
    let command_preview = format!(
        "{} {}",
        executable.to_string_lossy(),
        specification.args.join(" ")
    );
    let plan = PackageActionPlan {
        id: id.clone(),
        manager_id: PackageManagerId::Homebrew,
        action,
        targets: specification.targets,
        command_preview,
        warnings,
        preview_lines,
        requires_network: matches!(action, PackageAction::Install | PackageAction::Upgrade),
        created_at: Utc::now().to_rfc3339(),
    };
    registry.register(RegisteredPlan {
        plan: plan.clone(),
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
    run_action_process(
        &registered.executable,
        &registered.args,
        cancelled,
        ACTION_TIMEOUT,
        on_log,
    )
}

#[derive(Debug)]
struct HomebrewSpec {
    targets: Vec<String>,
    args: Vec<String>,
    warnings: Vec<String>,
}

fn build_homebrew_spec(
    action: PackageAction,
    targets: Vec<String>,
    installed: &[&str],
) -> Result<HomebrewSpec, AppError> {
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
    Ok(HomebrewSpec {
        targets: normalized,
        args,
        warnings,
    })
}

fn validate_homebrew_executable(executable: &Path) -> Result<(), AppError> {
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

fn is_valid_formula_name(value: &str) -> bool {
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
    cancelled: &AtomicBool,
    timeout: Duration,
    on_log: &dyn Fn(String),
) -> ActionExecution {
    let mut child = match Command::new(executable)
        .args(args)
        .env("NO_COLOR", "1")
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
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
                    Some(format!("Homebrew 退出码：{}", exit.code().unwrap_or(-1))),
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
            requires_network: false,
            created_at: Utc::now().to_rfc3339(),
        };
        registry
            .register(RegisteredPlan {
                plan,
                executable: "/bin/echo".into(),
                args: vec!["cleanup".into()],
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
            requires_network: false,
            created_at: (Utc::now() - chrono::Duration::minutes(11)).to_rfc3339(),
        };
        registry
            .register(RegisteredPlan {
                plan,
                executable: "/bin/echo".into(),
                args: vec!["cleanup".into()],
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
            &cancelled,
            Duration::from_secs(2),
            &|_| {},
        );
        canceller.join().unwrap();
        assert_eq!(result.status, PackageActionStatus::Unknown);
        assert!(result.error.unwrap().contains("状态未知"));
    }
}
