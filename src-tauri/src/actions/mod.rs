//! 受控写操作内核：一次性计划注册表与公共类型；具体逻辑见 plan/execute/specs/validation 子模块。
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

mod capabilities;
pub mod catalog;
mod reconcile;

pub use capabilities::package_action_capabilities;
pub use reconcile::{reconcile_observed_outcome, reconcile_pending_audits};

use crate::{
    error::AppError,
    models::{PackageActionPlan, PackageActionStatus},
};

const PLAN_TTL_SECONDS: i64 = 10 * 60;
const MAX_BATCH_TARGETS: usize = 20;
const MAX_PROGRESS_LINES: usize = 200;
const MAX_ACTION_LOG_BYTES: usize = 32_000;
const ACTION_TIMEOUT: Duration = Duration::from_secs(30 * 60);

mod execute;
mod plan;
mod specs;
mod validation;

pub use execute::execute_registered_plan;
#[allow(unused_imports)]
use execute::*;
pub use plan::create_package_plan;
#[allow(unused_imports)]
use plan::*;
#[allow(unused_imports)]
use specs::*;
#[allow(unused_imports)]
use validation::*;

#[derive(Debug, Clone)]
pub struct RegisteredPlan {
    pub plan: PackageActionPlan,
    pub executable: PathBuf,
    pub args: Vec<String>,
    environment: Vec<(String, String)>,
    executable_fingerprint: Option<ExecutableFingerprint>,
    npm_context: Option<NpmExecutionContext>,
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
            return Err(AppError::ActionConflict);
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

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::Ordering;

    use std::thread;

    use super::execute::{execute_with_runner, run_action_process, PackageActionRunner};
    use super::plan::{parse_homebrew_dependents, plan_is_fresh, preview_output_lines};
    use super::specs::{build_homebrew_spec, build_npm_spec, build_pnpm_spec};
    use super::validation::{
        action_environment, capture_executable_fingerprint, is_allowed_npm_data_path,
        is_valid_formula_name, is_valid_registry_package_name, validate_homebrew_executable,
        validate_npm_executable, validate_pnpm_executable, verify_executable_fingerprint,
    };
    use super::*;
    use crate::models::{PackageAction, PackageManagerId};
    use chrono::Utc;

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
    fn fingerprint_detects_same_size_same_mtime_content_swap() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("tool");
        std::fs::write(&executable, "payload-A").unwrap();
        let before = capture_executable_fingerprint(&executable).unwrap();
        let original_mtime = std::fs::metadata(&executable).unwrap().modified().unwrap();

        std::fs::write(&executable, "payload-B").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&executable)
            .unwrap()
            .set_modified(original_mtime)
            .unwrap();

        let metadata = std::fs::metadata(&executable).unwrap();
        assert_eq!(metadata.len(), before.size);
        assert_eq!(metadata.modified().unwrap(), original_mtime);
        assert!(verify_executable_fingerprint(&executable, &before).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn fingerprint_rejects_world_writable_executables() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("tool");
        std::fs::write(&executable, "content").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(capture_executable_fingerprint(&executable).is_err());

        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(capture_executable_fingerprint(&executable).is_ok());
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
