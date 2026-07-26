//! 计划构建：目标校验、预检执行与一次性计划生成。
use std::{
    io::{BufRead, Read},
    path::PathBuf,
    sync::atomic::AtomicBool,
};

use chrono::Utc;
use uuid::Uuid;

use crate::{
    adapters::runner::{redact_and_truncate, CommandRunner},
    error::AppError,
    models::{ExecutionTrust, ManagerStatus, PackageAction, PackageActionPlan, PackageManagerId},
    storage::Storage,
};

use super::specs::{build_homebrew_spec, build_npm_spec, build_pnpm_spec};
use super::validation::{
    capture_executable_fingerprint, readable_action_path, NpmExecutionContext,
};
use super::*;

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
        return Err(AppError::RecoveryRequired);
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

pub(super) fn plan_is_fresh(plan: &PackageActionPlan) -> bool {
    chrono::DateTime::parse_from_rfc3339(&plan.created_at)
        .ok()
        .is_some_and(|created| {
            Utc::now().signed_duration_since(created) <= chrono::Duration::seconds(PLAN_TTL_SECONDS)
        })
}

pub(super) fn preview_output_lines(stdout: &str, stderr: &str) -> Vec<String> {
    [stdout, stderr]
        .into_iter()
        .flat_map(str::lines)
        .map(redact_and_truncate)
        .filter(|line| !line.is_empty())
        .take(20)
        .collect()
}

pub(super) fn parse_homebrew_dependents(output: &str) -> Vec<String> {
    output
        .split_whitespace()
        .filter(|value| is_valid_formula_name(value))
        .map(str::to_string)
        .collect()
}
