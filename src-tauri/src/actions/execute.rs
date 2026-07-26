//! 计划执行：指纹复验、受控进程运行与输出流转。
use std::{
    io::{BufRead, BufReader, Read},
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use crate::{
    adapters::runner::{redact_and_truncate, CommandRunner},
    error::AppError,
    models::{PackageActionStatus, PackageManagerId},
};

use super::validation::{
    capture_executable_fingerprint, verify_executable_fingerprint, NpmExecutionContext,
};
use super::*;

pub(super) trait PackageActionRunner {
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

pub fn execute_registered_plan(
    registered: &RegisteredPlan,
    cancelled: &AtomicBool,
    on_log: &dyn Fn(String),
) -> ActionExecution {
    let span = tracing::info_span!(
        "package_action",
        manager = ?registered.plan.manager_id,
        action = ?registered.plan.action,
        targets = registered.plan.targets.len(),
    );
    let _entered = span.enter();
    let started_at = std::time::Instant::now();
    let execution = execute_registered_plan_inner(registered, cancelled, on_log);
    tracing::info!(
        status = ?execution.status,
        duration_ms = started_at.elapsed().as_millis() as u64,
        "受控写操作结束"
    );
    execution
}

fn execute_registered_plan_inner(
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

pub(super) fn execute_with_runner(
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

pub(super) fn run_action_process(
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

pub(super) fn stream_lines<R: Read + Send + 'static>(
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

pub(super) fn drain_progress(
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
