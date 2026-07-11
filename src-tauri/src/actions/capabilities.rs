use std::path::PathBuf;

use crate::{
    adapters::runner::CommandRunner,
    error::AppError,
    models::{
        ActionBlockerCode, ActionCapability, ActionCheckStatus, ActionPreflightCheck,
        ExecutionTrust, ManagerStatus, PackageAction, PackageManagerId,
    },
    storage::Storage,
};

use super::{
    npm_preflight, path_looks_read_only, validate_homebrew_executable, validate_npm_active_node,
    validate_npm_executable, validate_pnpm_executable,
};

const MANAGERS: [PackageManagerId; 3] = [
    PackageManagerId::Homebrew,
    PackageManagerId::Npm,
    PackageManagerId::Pnpm,
];
const ACTIONS: [PackageAction; 4] = [
    PackageAction::Install,
    PackageAction::Upgrade,
    PackageAction::Uninstall,
    PackageAction::Cleanup,
];

pub fn package_action_capabilities(storage: &Storage) -> Result<Vec<ActionCapability>, AppError> {
    let snapshot = storage.latest_snapshot()?;
    let recovery_required = storage.has_incomplete_action()?;
    let mut capabilities = Vec::new();
    for manager_id in MANAGERS {
        let mut base_checks = Vec::new();
        if !cfg!(target_os = "macos") {
            base_checks.push(blocked(
                ActionBlockerCode::UnsupportedPlatform,
                "平台不受支持",
                "受控写操作当前仅支持 macOS。",
            ));
        }
        if recovery_required {
            base_checks.push(blocked(
                ActionBlockerCode::RecoveryRequired,
                "需要恢复上次操作",
                "请先重新扫描并核对上次可能中断的操作。",
            ));
        }
        match snapshot.as_ref() {
            None => base_checks.push(blocked(
                ActionBlockerCode::MissingScan,
                "缺少环境扫描",
                "请先完成一次环境扫描。",
            )),
            Some(snapshot) => match snapshot
                .managers
                .iter()
                .find(|manager| manager.id == manager_id)
            {
                None => base_checks.push(blocked(
                    ActionBlockerCode::ManagerUnavailable,
                    "未发现包管理器",
                    &format!("扫描结果中没有 {}。", manager_id.as_str()),
                )),
                Some(manager) if !matches!(manager.status, ManagerStatus::Available) => {
                    base_checks.push(blocked(
                        ActionBlockerCode::ManagerUnavailable,
                        "包管理器不可用",
                        "当前扫描状态不允许执行写操作。",
                    ));
                }
                Some(manager) => {
                    evaluate_manager(snapshot, manager_id, manager, &mut base_checks);
                }
            },
        }
        for action in ACTIONS {
            let mut checks = base_checks.clone();
            add_action_notes(manager_id, action, &mut checks);
            let ready = !checks
                .iter()
                .any(|check| check.status == ActionCheckStatus::Blocked);
            if ready {
                checks.insert(
                    0,
                    passed(
                        ActionBlockerCode::Ready,
                        "基础条件已满足",
                        "仍需生成一次性计划并完成二次确认。",
                    ),
                );
            }
            capabilities.push(ActionCapability {
                manager_id,
                action,
                ready,
                checks,
            });
        }
    }
    Ok(capabilities)
}

fn evaluate_manager(
    snapshot: &crate::models::EnvironmentScan,
    manager_id: PackageManagerId,
    manager: &crate::models::PackageManager,
    checks: &mut Vec<ActionPreflightCheck>,
) {
    let trust_allowed = match manager_id {
        PackageManagerId::Homebrew => manager.execution_trust == ExecutionTrust::Managed,
        PackageManagerId::Npm | PackageManagerId::Pnpm => matches!(
            manager.execution_trust,
            ExecutionTrust::Managed | ExecutionTrust::UserManaged
        ),
        _ => false,
    };
    if !trust_allowed {
        checks.push(blocked(
            ActionBlockerCode::UntrustedExecutable,
            "可执行文件不受信任",
            "写操作只允许受管理目录或已识别用户工具目录。",
        ));
        return;
    }
    let Some(executable) = manager.executable_path.as_deref().map(PathBuf::from) else {
        checks.push(blocked(
            ActionBlockerCode::UntrustedExecutable,
            "缺少可执行路径",
            "扫描结果没有可复验的绝对路径。",
        ));
        return;
    };
    let validation = match manager_id {
        PackageManagerId::Homebrew => validate_homebrew_executable(&executable),
        PackageManagerId::Pnpm => validate_pnpm_executable(&executable),
        PackageManagerId::Npm => validate_npm_executable(&executable)
            .and_then(|_| validate_npm_active_node(snapshot, &executable)),
        _ => unreachable!(),
    };
    if let Err(error) = validation {
        let code = if manager_id == PackageManagerId::Npm {
            ActionBlockerCode::RuntimeConflict
        } else {
            ActionBlockerCode::UntrustedExecutable
        };
        checks.push(blocked(code, "执行上下文校验失败", &error.to_string()));
        return;
    }
    if manager_id == PackageManagerId::Npm {
        match npm_preflight(&CommandRunner::default(), &executable) {
            Ok((_node, prefix, _cache)) => {
                if path_looks_read_only(&prefix) {
                    checks.push(warning(
                        ActionBlockerCode::PermissionRisk,
                        "global prefix 可能不可写",
                        "操作可能失败；应用不会请求 sudo。",
                    ));
                }
            }
            Err(error) => checks.push(blocked(
                ActionBlockerCode::UnsafeDataPath,
                "npm 数据路径不安全",
                &error.to_string(),
            )),
        }
    }
}

fn add_action_notes(
    manager_id: PackageManagerId,
    action: PackageAction,
    checks: &mut Vec<ActionPreflightCheck>,
) {
    if matches!(action, PackageAction::Install | PackageAction::Upgrade) {
        checks.push(warning(
            ActionBlockerCode::NetworkRequired,
            "执行阶段需要联网",
            "计划本身不会联网，确认执行后包管理器可能访问软件源。",
        ));
    }
    if manager_id != PackageManagerId::Homebrew
        && matches!(
            action,
            PackageAction::Install | PackageAction::Upgrade | PackageAction::Uninstall
        )
    {
        checks.push(passed(
            ActionBlockerCode::ScriptsDisabled,
            "生命周期脚本已禁用",
            "固定使用 --ignore-scripts，部分 CLI 功能可能不完整。",
        ));
    }
    if action == PackageAction::Cleanup {
        let detail = match manager_id {
            PackageManagerId::Homebrew => "执行 brew cleanup，并在计划阶段使用 dry-run 预览。",
            PackageManagerId::Npm => "只执行 npm cache verify，不强制清空缓存。",
            PackageManagerId::Pnpm => "只执行 pnpm store prune，不直接删除目录。",
            _ => unreachable!(),
        };
        checks.push(warning(
            ActionBlockerCode::CacheSemantics,
            "缓存操作语义",
            detail,
        ));
    }
}

fn passed(code: ActionBlockerCode, title: &str, detail: &str) -> ActionPreflightCheck {
    check(code, ActionCheckStatus::Pass, title, detail)
}

fn warning(code: ActionBlockerCode, title: &str, detail: &str) -> ActionPreflightCheck {
    check(code, ActionCheckStatus::Warning, title, detail)
}

fn blocked(code: ActionBlockerCode, title: &str, detail: &str) -> ActionPreflightCheck {
    check(code, ActionCheckStatus::Blocked, title, detail)
}

fn check(
    code: ActionBlockerCode,
    status: ActionCheckStatus,
    title: &str,
    detail: &str,
) -> ActionPreflightCheck {
    ActionPreflightCheck {
        code,
        status,
        title: title.into(),
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn reports_stable_missing_scan_blockers_for_every_action() {
        let directory = tempdir().unwrap();
        let storage = Storage::at(directory.path().join("test.sqlite3")).unwrap();
        let capabilities = package_action_capabilities(&storage).unwrap();
        assert_eq!(capabilities.len(), MANAGERS.len() * ACTIONS.len());
        assert!(capabilities.iter().all(|capability| !capability.ready));
        assert!(capabilities.iter().all(|capability| capability
            .checks
            .iter()
            .any(|check| check.code == ActionBlockerCode::MissingScan
                && check.status == ActionCheckStatus::Blocked)));
    }
}
