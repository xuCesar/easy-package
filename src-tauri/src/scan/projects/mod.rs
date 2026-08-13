//! 项目扫描入口：目录遍历、扫描设置与各子模块的编排。
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use chrono::Utc;
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use crate::{
    error::AppError,
    models::{
        DependencyInsight, LogCategory, LogStatus, ProjectAnalysis, ProjectMetadata,
        ProjectWorkspace, ScanSettings, TaskLog,
    },
};

mod insights;
mod lockfile;
mod parse;
mod workspace;

use insights::dependency_insights;
use parse::parse_project;
use workspace::discover_workspaces;

const MARKERS: [&str; 21] = [
    "package.json",
    "pyproject.toml",
    "requirements.txt",
    "Pipfile",
    "Pipfile.lock",
    "poetry.lock",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "package-lock.json",
    "uv.lock",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
    "Cargo.toml",
    "Cargo.lock",
    "go.mod",
    "go.sum",
    "Gemfile",
    "Gemfile.lock",
    "composer.json",
    "composer.lock",
];
const MIN_MAX_DEPTH: usize = 1;
const MAX_MAX_DEPTH: usize = 12;

#[derive(Debug)]
pub struct ProjectScan {
    pub projects: Vec<ProjectMetadata>,
    pub dependency_insights: Vec<DependencyInsight>,
    pub workspaces: Vec<ProjectWorkspace>,
    pub logs: Vec<TaskLog>,
    pub failures: usize,
}

#[cfg(test)]
pub fn scan_projects(roots: &[PathBuf], cancelled: &AtomicBool) -> Result<ProjectScan, AppError> {
    scan_projects_with_settings(roots, &ScanSettings::default(), cancelled)
}

pub fn scan_projects_with_settings(
    roots: &[PathBuf],
    settings: &ScanSettings,
    cancelled: &AtomicBool,
) -> Result<ProjectScan, AppError> {
    let mut directories: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
    let mut logs = Vec::new();
    let mut failures = 0;

    for root in roots {
        if !root.is_dir() {
            failures += 1;
            logs.push(project_log(
                LogStatus::Error,
                &format!("扫描目录不存在：{}", root.display()),
            ));
            continue;
        }
        let skipped = RefCell::new(BTreeSet::new());
        for entry in WalkDir::new(root)
            .max_depth(settings.max_depth)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| should_visit(entry, settings, &skipped))
        {
            if cancelled.load(Ordering::SeqCst) {
                return Err(AppError::ScanCancelled);
            }
            match entry {
                Ok(entry) if entry.file_type().is_file() => {
                    let file_name = entry.file_name().to_string_lossy();
                    if MARKERS.contains(&file_name.as_ref()) {
                        if let Some(parent) = entry.path().parent() {
                            directories
                                .entry(parent.to_path_buf())
                                .or_default()
                                .insert(file_name.into_owned());
                        }
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    failures += 1;
                    logs.push(project_log(
                        LogStatus::Warning,
                        &format!("部分目录无法读取：{}", error),
                    ));
                }
            }
        }
        for message in skipped.into_inner() {
            logs.push(project_log(LogStatus::Info, &message));
        }
    }

    let mut projects = directories
        .into_iter()
        .map(|(directory, markers)| parse_project(&directory, &markers))
        .collect::<Vec<_>>();
    let workspaces = discover_workspaces(&mut projects);
    if !roots.is_empty() {
        logs.push(project_log(
            LogStatus::Success,
            &format!("项目扫描完成，共识别 {} 个项目", projects.len()),
        ));
    }
    let dependency_insights = dependency_insights(&projects);
    Ok(ProjectScan {
        projects,
        dependency_insights,
        workspaces,
        logs,
        failures,
    })
}

pub fn analyze_projects(
    roots: &[PathBuf],
    settings: &ScanSettings,
    cancelled: &AtomicBool,
    previous_projects: &[ProjectMetadata],
) -> Result<ProjectAnalysis, AppError> {
    let mut scan = scan_projects_with_settings(roots, settings, cancelled)?;
    super::dependency_graph::reuse_dependency_graph_summaries(
        &mut scan.projects,
        cancelled,
        previous_projects,
    )?;
    Ok(ProjectAnalysis {
        projects: scan.projects,
        dependency_insights: scan.dependency_insights,
        workspaces: scan.workspaces,
        runtime_assessments: Vec::new(),
        health_issues: Vec::new(),
        scan_settings: settings.clone(),
    })
}

fn should_visit(
    entry: &DirEntry,
    settings: &ScanSettings,
    skipped: &RefCell<BTreeSet<String>>,
) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    if !entry.file_type().is_dir() {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    if settings
        .default_ignored_directory_names
        .iter()
        .any(|ignored| ignored == name.as_ref())
    {
        skipped.borrow_mut().insert(format!(
            "跳过默认忽略目录：{}（{}）",
            name,
            entry.path().display()
        ));
        return false;
    }
    if let Some(ignored) = settings
        .ignored_paths
        .iter()
        .find(|ignored| entry.path().starts_with(Path::new(ignored)))
    {
        skipped
            .borrow_mut()
            .insert(format!("跳过用户忽略目录：{}", ignored));
        return false;
    }
    true
}

pub fn normalize_scan_settings(
    mut settings: ScanSettings,
    roots: &[PathBuf],
) -> Result<ScanSettings, AppError> {
    if !(MIN_MAX_DEPTH..=MAX_MAX_DEPTH).contains(&settings.max_depth) {
        return Err(AppError::InvalidScanSettings(format!(
            "最大扫描深度需在 {MIN_MAX_DEPTH} 到 {MAX_MAX_DEPTH} 之间"
        )));
    }
    settings.default_ignored_directory_names = crate::models::default_ignored_directory_names();
    let mut ignored_paths = BTreeSet::new();
    for raw_path in settings.ignored_paths {
        let path = PathBuf::from(raw_path.trim());
        if raw_path.trim().is_empty() || !path.is_absolute() || !path.is_dir() {
            return Err(AppError::InvalidScanSettings(
                "忽略目录必须是存在的绝对目录".into(),
            ));
        }
        let canonical = path
            .canonicalize()
            .map_err(|error| AppError::InvalidScanSettings(error.to_string()))?;
        if !roots.iter().any(|root| canonical.starts_with(root)) {
            return Err(AppError::InvalidScanSettings(
                "忽略目录必须位于已添加的扫描目录内".into(),
            ));
        }
        if roots.iter().any(|root| canonical == *root) {
            return Err(AppError::InvalidScanSettings(
                "不能将扫描根目录本身设为忽略目录".into(),
            ));
        }
        ignored_paths.insert(canonical.to_string_lossy().into_owned());
    }
    settings.ignored_paths = ignored_paths.into_iter().collect();
    Ok(settings)
}

fn project_log(status: LogStatus, message: &str) -> TaskLog {
    TaskLog {
        id: Uuid::new_v4().to_string(),
        category: LogCategory::Project,
        status,
        message: message.into(),
        manager_id: None,
        exit_code: None,
        output: None,
        timestamp: Utc::now().to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn scans_project_metadata_and_ignores_node_modules() {
        let root = tempdir().unwrap();
        let project = root.path().join("demo");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("package.json"),
            r#"{"name":"demo-app","packageManager":"pnpm@11","engines":{"node":">=22"}}"#,
        )
        .unwrap();
        fs::write(project.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'").unwrap();
        let ignored = project.join("node_modules/hidden");
        fs::create_dir_all(&ignored).unwrap();
        fs::write(ignored.join("package.json"), r#"{"name":"hidden"}"#).unwrap();
        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].name, "demo-app");
        assert_eq!(
            result.projects[0].runtime_requirements[0].requirement,
            ">=22"
        );
        assert!(result
            .logs
            .iter()
            .any(|log| log.message.contains("跳过默认忽略目录：node_modules")));
    }

    #[test]
    fn ignores_next_build_output_as_projects() {
        let root = tempdir().unwrap();
        let project = root.path().join("web");
        let generated = project.join(".next/types");
        fs::create_dir_all(&generated).unwrap();
        fs::write(project.join("package.json"), r#"{"name":"web"}"#).unwrap();
        fs::write(
            generated.join("package.json"),
            r#"{"name":"generated-types"}"#,
        )
        .unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();

        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].name, "web");
        assert!(result
            .logs
            .iter()
            .any(|log| log.message.contains("跳过默认忽略目录：.next")));
    }

    #[test]
    fn honors_user_ignored_paths_and_reports_them_in_logs() {
        let root = tempdir().unwrap();
        let ignored = root.path().join("generated");
        fs::create_dir_all(&ignored).unwrap();
        fs::write(ignored.join("package.json"), r#"{"name":"ignored"}"#).unwrap();
        let settings = ScanSettings {
            ignored_paths: vec![ignored.to_string_lossy().into_owned()],
            ..ScanSettings::default()
        };

        let result = scan_projects_with_settings(
            &[root.path().to_path_buf()],
            &settings,
            &AtomicBool::new(false),
        )
        .unwrap();

        assert!(result.projects.is_empty());
        assert!(result
            .logs
            .iter()
            .any(|log| log.message.contains("跳过用户忽略目录")));
    }

    #[test]
    fn max_depth_limits_project_marker_discovery() {
        let root = tempdir().unwrap();
        let deep_project = root.path().join("one/two/three/four/five/six");
        fs::create_dir_all(&deep_project).unwrap();
        fs::write(deep_project.join("package.json"), r#"{"name":"deep"}"#).unwrap();

        let shallow = ScanSettings {
            max_depth: 6,
            ..ScanSettings::default()
        };
        assert!(scan_projects_with_settings(
            &[root.path().to_path_buf()],
            &shallow,
            &AtomicBool::new(false),
        )
        .unwrap()
        .projects
        .is_empty());

        let deeper = ScanSettings {
            max_depth: 7,
            ..ScanSettings::default()
        };
        assert_eq!(
            scan_projects_with_settings(
                &[root.path().to_path_buf()],
                &deeper,
                &AtomicBool::new(false),
            )
            .unwrap()
            .projects
            .len(),
            1
        );
    }

    #[test]
    fn rejects_ignored_paths_outside_scan_roots_and_invalid_depth() {
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let error = normalize_scan_settings(
            ScanSettings {
                ignored_paths: vec![outside.path().to_string_lossy().into_owned()],
                ..ScanSettings::default()
            },
            &[root.path().to_path_buf()],
        )
        .unwrap_err();
        assert!(matches!(error, AppError::InvalidScanSettings(_)));

        let error = normalize_scan_settings(
            ScanSettings {
                max_depth: 0,
                ..ScanSettings::default()
            },
            &[root.path().to_path_buf()],
        )
        .unwrap_err();
        assert!(matches!(error, AppError::InvalidScanSettings(_)));
    }

    #[test]
    fn reports_lockfile_mismatch() {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("package.json"),
            r#"{"name":"demo","packageManager":"pnpm@11"}"#,
        )
        .unwrap();
        fs::write(root.path().join("package-lock.json"), "{}").unwrap();
        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        assert_eq!(result.projects[0].warnings.len(), 1);
    }

    #[test]
    fn reports_yarn_and_bun_lockfile_mismatches() {
        let root = tempdir().unwrap();
        let yarn = root.path().join("yarn-project");
        let bun = root.path().join("bun-project");
        fs::create_dir_all(&yarn).unwrap();
        fs::create_dir_all(&bun).unwrap();
        fs::write(yarn.join("package.json"), r#"{"packageManager":"yarn@4"}"#).unwrap();
        fs::write(yarn.join("package-lock.json"), "{}").unwrap();
        fs::write(bun.join("package.json"), r#"{"packageManager":"bun@1"}"#).unwrap();
        fs::write(bun.join("yarn.lock"), "").unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        assert!(result.projects.iter().all(|project| project
            .warnings
            .iter()
            .any(|warning| warning.contains("未发现对应"))));
    }

    #[test]
    fn reads_rust_project_metadata() {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname = \"toolbox\"\nrust-version = \"1.84\"\n",
        )
        .unwrap();
        fs::write(root.path().join("Cargo.lock"), "version = 4").unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();

        assert_eq!(result.projects[0].name, "toolbox");
        assert_eq!(result.projects[0].package_manager.as_deref(), Some("cargo"));
        assert_eq!(result.projects[0].runtime_requirements[0].runtime, "Rust");
    }

    #[test]
    fn stops_when_scan_is_cancelled() {
        let root = tempdir().unwrap();
        let cancelled = AtomicBool::new(true);

        let error = scan_projects(&[root.path().to_path_buf()], &cancelled).unwrap_err();

        assert!(matches!(error, AppError::ScanCancelled));
    }

    #[test]
    fn cold_project_analysis_does_not_build_complete_graphs() {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("package.json"),
            r#"{"name":"cold-app","packageManager":"npm@11","dependencies":{"react":"^19"}}"#,
        )
        .unwrap();
        // 完整图解析会将这个锁文件标记为 Invalid；冷项目扫描仍应只返回轻量元数据。
        fs::write(root.path().join("package-lock.json"), "{invalid-json").unwrap();

        let analysis = analyze_projects(
            &[root.path().to_path_buf()],
            &ScanSettings::default(),
            &AtomicBool::new(false),
            &[],
        )
        .unwrap();

        assert_eq!(analysis.projects.len(), 1);
        assert!(analysis.projects[0].dependency_graph_summary.is_none());
        assert!(analysis.projects[0].supply_chain_risk_summary.is_none());
    }

    #[test]
    fn reports_ambiguous_node_lockfiles() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("package.json"), "{}").unwrap();
        fs::write(root.path().join("yarn.lock"), "").unwrap();
        fs::write(root.path().join("bun.lock"), "").unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();

        assert!(result.projects[0].package_manager.is_none());
        assert_eq!(result.projects[0].warnings.len(), 1);
    }

    #[test]
    fn indexes_direct_dependencies_across_ecosystems_and_scopes() {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("package.json"),
            r#"{"dependencies":{"react":"^19"},"devDependencies":{"react":"^19","vitest":"^3"}}"#,
        )
        .unwrap();
        fs::write(
            root.path().join("pyproject.toml"),
            "[project]\ndependencies = [\"HTTPX>=0.28\", \"invalid requirement!\"]\n",
        )
        .unwrap();
        fs::write(
            root.path().join("requirements.txt"),
            "pydantic>=2\n-r nested.txt\nhttps://example.com/a.whl\n",
        )
        .unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            "[dependencies]\nserde = \"1\"\n[dev-dependencies]\nserde = { version = \"1\" }\n",
        )
        .unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        let dependencies = &result.projects[0].dependencies;
        let react = dependencies
            .iter()
            .find(|item| item.name == "react")
            .unwrap();
        assert_eq!(react.scopes, vec!["开发", "运行"]);
        assert!(dependencies
            .iter()
            .any(|item| item.ecosystem == "Python" && item.normalized_name == "httpx"));
        assert!(dependencies
            .iter()
            .any(|item| item.ecosystem == "Rust" && item.name == "serde"));
        assert!(!dependencies
            .iter()
            .any(|item| item.name.contains("invalid")));
    }

    #[test]
    fn keeps_same_name_isolated_by_ecosystem_and_detects_versions() {
        let root = tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(
            first.join("package.json"),
            r#"{"dependencies":{"shared":"^1"}}"#,
        )
        .unwrap();
        fs::write(
            first.join("pyproject.toml"),
            "[project]\ndependencies = [\"shared>=2\"]",
        )
        .unwrap();
        fs::write(
            second.join("package.json"),
            r#"{"dependencies":{"shared":"^2"}}"#,
        )
        .unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        assert_eq!(result.dependency_insights.len(), 2);
        let javascript = result
            .dependency_insights
            .iter()
            .find(|item| item.ecosystem == "JavaScript")
            .unwrap();
        assert!(javascript.has_version_divergence);
        assert_eq!(javascript.project_count, 2);
    }

    #[test]
    fn recognizes_javascript_and_rust_workspaces() {
        let root = tempdir().unwrap();
        let javascript_member = root.path().join("apps/web");
        let rust_member = root.path().join("crates/cli");
        fs::create_dir_all(&javascript_member).unwrap();
        fs::create_dir_all(&rust_member).unwrap();
        fs::write(
            root.path().join("package.json"),
            r#"{"name":"web-suite","workspaces":["apps/*"]}"#,
        )
        .unwrap();
        fs::write(javascript_member.join("package.json"), r#"{"name":"web"}"#).unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
        fs::write(
            rust_member.join("Cargo.toml"),
            "[package]\nname = \"cli\"\nversion = \"0.1.0\"",
        )
        .unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        assert_eq!(result.workspaces.len(), 2);
        assert_eq!(
            result
                .projects
                .iter()
                .find(|project| project.name == "web")
                .unwrap()
                .workspace
                .as_ref()
                .unwrap()
                .name,
            "web-suite"
        );
        assert_eq!(
            result
                .projects
                .iter()
                .find(|project| project.name == "cli")
                .unwrap()
                .workspace
                .as_ref()
                .unwrap()
                .ecosystem,
            "Rust"
        );
    }

    #[test]
    fn recognizes_pnpm_workspace_file_without_root_package_manifest() {
        let root = tempdir().unwrap();
        let member = root.path().join("packages/ui");
        fs::create_dir_all(&member).unwrap();
        fs::write(
            root.path().join("pnpm-workspace.yaml"),
            "packages:\n  - 'packages/*'\n",
        )
        .unwrap();
        fs::write(member.join("package.json"), r#"{"name":"ui"}"#).unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        assert_eq!(result.workspaces.len(), 1);
        assert_eq!(
            result.workspaces[0].member_paths,
            vec![member.to_string_lossy()]
        );
    }

    #[test]
    fn resolves_direct_dependencies_from_supported_lockfiles() {
        let root = tempdir().unwrap();
        let npm = root.path().join("npm");
        let pnpm = root.path().join("pnpm");
        let cargo = root.path().join("cargo");
        let uv = root.path().join("uv");
        for directory in [&npm, &pnpm, &cargo, &uv] {
            fs::create_dir_all(directory).unwrap();
        }
        fs::write(
            npm.join("package.json"),
            r#"{"name":"npm-app","dependencies":{"react":"^19"}}"#,
        )
        .unwrap();
        fs::write(npm.join("package-lock.json"), r#"{"lockfileVersion":3,"packages":{"":{"name":"npm-app","dependencies":{"react":"^19"}},"node_modules/react":{"version":"19.1.1"}}}"#).unwrap();
        fs::write(
            pnpm.join("package.json"),
            r#"{"name":"pnpm-app","dependencies":{"zod":"^3"}}"#,
        )
        .unwrap();
        fs::write(pnpm.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\nimporters:\n  .:\n    dependencies:\n      zod:\n        version: 3.24.1\n").unwrap();
        fs::write(
            cargo.join("Cargo.toml"),
            "[package]\nname = \"cargo-app\"\nversion = \"0.1.0\"\n[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();
        fs::write(cargo.join("Cargo.lock"), "[[package]]\nname = \"cargo-app\"\nversion = \"0.1.0\"\ndependencies = [\"serde 1.0.218\"]\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.218\"\n").unwrap();
        fs::write(
            uv.join("pyproject.toml"),
            "[project]\nname = \"uv-app\"\ndependencies = [\"httpx>=0.28\"]\n",
        )
        .unwrap();
        fs::write(uv.join("uv.lock"), "[[package]]\nname = \"uv-app\"\nversion = \"0.1.0\"\ndependencies = [{ name = \"httpx\" }]\n\n[[package]]\nname = \"httpx\"\nversion = \"0.28.1\"\n").unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        let expected = [
            ("npm-app", "react", "19.1.1", "package-lock.json"),
            ("pnpm-app", "zod", "3.24.1", "pnpm-lock.yaml"),
            ("cargo-app", "serde", "1.0.218", "Cargo.lock"),
            ("uv-app", "httpx", "0.28.1", "uv.lock"),
        ];
        for (project_name, dependency_name, version, source) in expected {
            let dependency = result
                .projects
                .iter()
                .find(|project| project.name == project_name)
                .unwrap()
                .dependencies
                .iter()
                .find(|dependency| dependency.name == dependency_name)
                .unwrap();
            assert_eq!(dependency.resolved_version.as_deref(), Some(version));
            assert_eq!(dependency.resolution_source.as_deref(), Some(source));
        }
    }

    #[test]
    fn resolves_yarn_and_bun_text_lockfiles_without_parsing_bun_lockb() {
        let root = tempdir().unwrap();
        let yarn_classic = root.path().join("yarn-classic");
        let yarn_berry = root.path().join("yarn-berry");
        let yarn_workspace = root.path().join("yarn-workspace");
        let yarn_member = yarn_workspace.join("packages/web");
        let bun = root.path().join("bun");
        let bun_binary = root.path().join("bun-binary");
        for directory in [&yarn_classic, &yarn_berry, &yarn_member, &bun, &bun_binary] {
            fs::create_dir_all(directory).unwrap();
        }
        fs::write(
            yarn_classic.join("package.json"),
            r#"{"name":"classic","dependencies":{"lodash":"^4.17.0"}}"#,
        )
        .unwrap();
        fs::write(
            yarn_classic.join("yarn.lock"),
            "# yarn lockfile v1\n\nlodash@^4.17.0:\n  version \"4.17.21\"\n",
        )
        .unwrap();
        fs::write(
            yarn_berry.join("package.json"),
            r#"{"name":"berry","packageManager":"yarn@4.6.0","dependencies":{"react":"^19.0.0"}}"#,
        )
        .unwrap();
        fs::write(
            yarn_berry.join("yarn.lock"),
            "__metadata:\n  version: 8\n\n\"react@npm:^19.0.0\":\n  version: 19.1.1\n",
        )
        .unwrap();
        fs::write(
            yarn_workspace.join("package.json"),
            r#"{"name":"suite","packageManager":"yarn@4.6.0","workspaces":["packages/*"]}"#,
        )
        .unwrap();
        fs::write(
            yarn_workspace.join("yarn.lock"),
            "\"zod@npm:^3.0.0\":\n  version: 3.24.1\n",
        )
        .unwrap();
        fs::write(
            yarn_member.join("package.json"),
            r#"{"name":"web","dependencies":{"zod":"^3.0.0"}}"#,
        )
        .unwrap();
        fs::write(
            bun.join("package.json"),
            r#"{"name":"bun-app","packageManager":"bun@1.3.0","dependencies":{"hono":"^4.6.0"}}"#,
        )
        .unwrap();
        fs::write(
            bun.join("bun.lock"),
            r#"{"lockfileVersion":0,"packages":{"hono":["hono@4.6.14","",{},""]}}"#,
        )
        .unwrap();
        fs::write(
            bun_binary.join("package.json"),
            r#"{"name":"bun-binary","packageManager":"bun@1.0.0","dependencies":{"zod":"^3.0.0"}}"#,
        )
        .unwrap();
        fs::write(bun_binary.join("bun.lockb"), [0_u8, 1, 2, 3]).unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        let resolved = [
            ("classic", "lodash", "4.17.21", "yarn.lock"),
            ("berry", "react", "19.1.1", "yarn.lock"),
            ("web", "zod", "3.24.1", "yarn.lock"),
            ("bun-app", "hono", "4.6.14", "bun.lock"),
        ];
        for (project_name, dependency_name, version, source) in resolved {
            let dependency = result
                .projects
                .iter()
                .find(|project| project.name == project_name)
                .unwrap()
                .dependencies
                .iter()
                .find(|dependency| dependency.name == dependency_name)
                .unwrap();
            assert_eq!(dependency.resolved_version.as_deref(), Some(version));
            assert_eq!(dependency.resolution_source.as_deref(), Some(source));
        }

        let binary = result
            .projects
            .iter()
            .find(|project| project.name == "bun-binary")
            .unwrap();
        assert!(binary.lock_files.contains(&"bun.lockb".into()));
        assert!(binary
            .warnings
            .iter()
            .any(|warning| warning.contains("bun.lockb 二进制")));
        assert!(!binary.dependencies[0].resolution_checked);
    }

    #[test]
    fn resolves_pnpm_workspace_importer_and_reports_unresolved_dependencies() {
        let root = tempdir().unwrap();
        let member = root.path().join("packages/web");
        fs::create_dir_all(&member).unwrap();
        fs::write(
            root.path().join("pnpm-workspace.yaml"),
            "packages:\n  - packages/*\n",
        )
        .unwrap();
        fs::write(
            member.join("package.json"),
            r#"{"name":"web","dependencies":{"react":"^19","missing":"^1"}}"#,
        )
        .unwrap();
        fs::write(root.path().join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\nimporters:\n  packages/web:\n    dependencies:\n      react:\n        version: 19.1.1\n").unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        let web = result
            .projects
            .iter()
            .find(|project| project.name == "web")
            .unwrap();
        assert_eq!(
            web.dependencies
                .iter()
                .find(|dependency| dependency.name == "react")
                .unwrap()
                .resolved_version
                .as_deref(),
            Some("19.1.1")
        );
        assert!(
            web.dependencies
                .iter()
                .find(|dependency| dependency.name == "missing")
                .unwrap()
                .resolution_checked
        );
        assert!(
            result
                .dependency_insights
                .iter()
                .find(|insight| insight.name == "missing")
                .unwrap()
                .has_resolution_risk
        );
    }

    #[test]
    fn resolves_poetry_pipenv_and_go_project_dependencies() {
        let root = tempdir().unwrap();
        let poetry = root.path().join("poetry");
        let pipenv = root.path().join("pipenv");
        let go = root.path().join("go");
        for directory in [&poetry, &pipenv, &go] {
            fs::create_dir_all(directory).unwrap();
        }
        fs::write(
            poetry.join("pyproject.toml"),
            "[tool.poetry]\nname = \"poetry-app\"\n[tool.poetry.dependencies]\npython = \">=3.12\"\nhttpx = \"^0.28\"\n[tool.poetry.group.dev.dependencies]\npytest = \"^8\"\n",
        )
        .unwrap();
        fs::write(
            poetry.join("poetry.lock"),
            "[[package]]\nname = \"httpx\"\nversion = \"0.28.1\"\n\n[[package]]\nname = \"pytest\"\nversion = \"8.3.4\"\n",
        )
        .unwrap();
        fs::write(
            pipenv.join("Pipfile"),
            "[packages]\nrequests = \"==2.32.3\"\n[dev-packages]\nblack = \"*\"\n",
        )
        .unwrap();
        fs::write(
            pipenv.join("Pipfile.lock"),
            r#"{"default":{"requests":{"version":"==2.32.3"}},"develop":{"black":{"version":"==24.10.0"}}}"#,
        )
        .unwrap();
        fs::write(
            go.join("go.mod"),
            "module example.com/tool\ngo 1.23\nrequire (\n  github.com/spf13/cobra v1.8.1\n  golang.org/x/text v0.20.0 // indirect\n)\n",
        )
        .unwrap();
        fs::write(go.join("go.sum"), "").unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        let poetry_project = result
            .projects
            .iter()
            .find(|project| project.name == "poetry-app")
            .unwrap();
        assert_eq!(
            poetry_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "httpx")
                .unwrap()
                .resolved_version
                .as_deref(),
            Some("0.28.1")
        );
        assert_eq!(
            poetry_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "pytest")
                .unwrap()
                .scopes,
            vec!["开发:dev"]
        );
        let pipenv_project = result
            .projects
            .iter()
            .find(|project| project.path == pipenv.to_string_lossy())
            .unwrap();
        assert_eq!(
            pipenv_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "black")
                .unwrap()
                .resolved_version
                .as_deref(),
            Some("24.10.0")
        );
        let go_project = result
            .projects
            .iter()
            .find(|project| project.name == "example.com/tool")
            .unwrap();
        assert_eq!(
            go_project
                .runtime_requirements
                .iter()
                .find(|item| item.runtime == "Go")
                .unwrap()
                .requirement,
            "1.23"
        );
        assert_eq!(
            go_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "github.com/spf13/cobra")
                .unwrap()
                .resolution_source
                .as_deref(),
            Some("go.mod")
        );
    }

    #[test]
    fn resolves_ruby_and_php_direct_dependencies_from_lockfiles() {
        let root = tempdir().unwrap();
        let ruby = root.path().join("ruby");
        let php = root.path().join("php");
        fs::create_dir_all(&ruby).unwrap();
        fs::create_dir_all(&php).unwrap();
        fs::write(
            ruby.join("Gemfile"),
            "gem \"rails\", \"~> 8.0\"\ngem \"rspec-rails\", \"~> 7.1\", group: :development\ngem \"missing-gem\", \"~> 1.0\"\n",
        )
        .unwrap();
        fs::write(ruby.join(".ruby-version"), "3.4.1\n").unwrap();
        fs::write(
            ruby.join("Gemfile.lock"),
            "GEM\n  specs:\n    rails (8.0.1)\n    rspec-rails (7.1.1)\n\nDEPENDENCIES\n  rails (~> 8.0)\n  rspec-rails (~> 7.1)\n",
        )
        .unwrap();
        fs::write(
            php.join("composer.json"),
            r#"{"name":"acme/api","require":{"php":"^8.3","symfony/http-foundation":"^7.2","ext-json":"*"},"require-dev":{"phpunit/phpunit":"^11.5","lib-icu":"*"}}"#,
        )
        .unwrap();
        fs::write(
            php.join("composer.lock"),
            r#"{"packages":[{"name":"symfony/http-foundation","version":"v7.2.1"}],"packages-dev":[{"name":"phpunit/phpunit","version":"11.5.3"}]}"#,
        )
        .unwrap();

        let result = scan_projects(&[root.path().to_path_buf()], &AtomicBool::new(false)).unwrap();
        let ruby_project = result
            .projects
            .iter()
            .find(|project| project.path == ruby.to_string_lossy())
            .unwrap();
        assert_eq!(ruby_project.package_manager.as_deref(), Some("bundler"));
        assert_eq!(
            ruby_project
                .runtime_requirements
                .iter()
                .find(|item| item.runtime == "Ruby")
                .unwrap()
                .requirement,
            "3.4.1"
        );
        assert_eq!(
            ruby_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "rails")
                .unwrap()
                .resolved_version
                .as_deref(),
            Some("8.0.1")
        );
        assert_eq!(
            ruby_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "rspec-rails")
                .unwrap()
                .scopes,
            vec!["开发"]
        );
        assert!(
            ruby_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "missing-gem")
                .unwrap()
                .resolution_checked
        );

        let php_project = result
            .projects
            .iter()
            .find(|project| project.name == "acme/api")
            .unwrap();
        assert_eq!(php_project.package_manager.as_deref(), Some("composer"));
        assert_eq!(
            php_project
                .runtime_requirements
                .iter()
                .find(|item| item.runtime == "PHP")
                .unwrap()
                .requirement,
            "^8.3"
        );
        assert_eq!(php_project.dependencies.len(), 2);
        assert_eq!(
            php_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "symfony/http-foundation")
                .unwrap()
                .resolution_source
                .as_deref(),
            Some("composer.lock")
        );
        assert_eq!(
            php_project
                .dependencies
                .iter()
                .find(|dependency| dependency.name == "phpunit/phpunit")
                .unwrap()
                .scopes,
            vec!["开发"]
        );
    }
}
