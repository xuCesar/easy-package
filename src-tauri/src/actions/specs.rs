//! 各管理器固定参数构建：Homebrew / npm / pnpm 白名单命令与参数。

use crate::{error::AppError, models::PackageAction};

use super::*;

#[derive(Debug)]
pub(super) struct ActionSpec {
    pub(super) targets: Vec<String>,
    pub(super) args: Vec<String>,
    pub(super) warnings: Vec<String>,
}

pub(super) fn build_homebrew_spec(
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

pub(super) fn build_pnpm_spec(
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

pub(super) fn build_npm_spec(
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
