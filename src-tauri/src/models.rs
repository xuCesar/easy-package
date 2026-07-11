use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageManagerId {
    Homebrew,
    Npm,
    Pnpm,
    Uv,
    Pip,
    Yarn,
    Bun,
    Cargo,
    Rubygems,
    Composer,
}

impl PackageManagerId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Homebrew => "homebrew",
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Uv => "uv",
            Self::Pip => "pip",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
            Self::Cargo => "cargo",
            Self::Rubygems => "rubygems",
            Self::Composer => "composer",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ManagerStatus {
    Available,
    Unavailable,
    Error,
    Blocked,
    Unsupported,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionTrust {
    System,
    Managed,
    UserManaged,
    Unverified,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CacheScanStatus {
    Complete,
    Partial,
    Unavailable,
    NotApplicable,
}

impl Default for CacheScanStatus {
    fn default() -> Self {
        Self::NotApplicable
    }
}

impl Default for ExecutionTrust {
    fn default() -> Self {
        Self::NotApplicable
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManager {
    pub id: PackageManagerId,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
    pub status: ManagerStatus,
    #[serde(default)]
    pub execution_trust: ExecutionTrust,
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<DiagnosticError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_size_bytes: Option<u64>,
    #[serde(default)]
    pub cache_scan_status: CacheScanStatus,
    pub scanned_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PackageScope {
    System,
    Global,
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateStatus {
    UpToDate,
    Available,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedPackage {
    pub id: String,
    pub manager_id: PackageManagerId,
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_version: Option<String>,
    pub scope: PackageScope,
    pub update_status: UpdateStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRequirement {
    pub runtime: String,
    pub requirement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMetadata {
    pub name: String,
    pub path: String,
    pub ecosystems: Vec<String>,
    pub lock_files: Vec<String>,
    pub runtime_requirements: Vec<RuntimeRequirement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_manager: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<ProjectDependency>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<ProjectWorkspaceRef>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorkspaceRef {
    pub name: String,
    pub path: String,
    pub ecosystem: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorkspace {
    pub name: String,
    pub path: String,
    pub ecosystem: String,
    pub member_paths: Vec<String>,
}

pub fn default_scan_max_depth() -> usize {
    6
}

pub fn default_ignored_directory_names() -> Vec<String> {
    [
        "node_modules",
        ".git",
        "target",
        "dist",
        "build",
        ".venv",
        "vendor",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NetworkPolicy {
    Offline,
    Registry,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self::Offline
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanSettings {
    #[serde(default)]
    pub ignored_paths: Vec<String>,
    #[serde(default = "default_scan_max_depth")]
    pub max_depth: usize,
    #[serde(default = "default_ignored_directory_names")]
    pub default_ignored_directory_names: Vec<String>,
    #[serde(default)]
    pub network_policy: NetworkPolicy,
}

impl Default for ScanSettings {
    fn default() -> Self {
        Self {
            ignored_paths: Vec::new(),
            max_depth: default_scan_max_depth(),
            default_ignored_directory_names: default_ignored_directory_names(),
            network_policy: NetworkPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInstallation {
    pub id: String,
    pub runtime: String,
    pub version: String,
    pub path: String,
    pub provider: String,
    pub is_active: bool,
    pub execution_trust: ExecutionTrust,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeRequirementStatus {
    Available,
    Missing,
    Mismatch,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRequirementAssessment {
    pub project_name: String,
    pub project_path: String,
    pub runtime: String,
    pub requirement: String,
    pub status: RuntimeRequirementStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_version: Option<String>,
    #[serde(default)]
    pub installed_versions: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDependency {
    pub ecosystem: String,
    pub name: String,
    pub normalized_name: String,
    pub version_requirement: String,
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_source: Option<String>,
    #[serde(default)]
    pub resolution_checked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyProjectUsage {
    pub project_name: String,
    pub project_path: String,
    pub version_requirement: String,
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyInsight {
    pub ecosystem: String,
    pub name: String,
    pub project_count: usize,
    pub version_requirements: Vec<String>,
    pub projects: Vec<DependencyProjectUsage>,
    pub has_version_divergence: bool,
    #[serde(default)]
    pub resolved_versions: Vec<String>,
    #[serde(default)]
    pub has_resolved_version_divergence: bool,
    #[serde(default)]
    pub has_resolution_risk: bool,
    #[serde(default)]
    pub has_health_risk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAnalysis {
    pub projects: Vec<ProjectMetadata>,
    pub dependency_insights: Vec<DependencyInsight>,
    pub workspaces: Vec<ProjectWorkspace>,
    #[serde(default)]
    pub runtime_assessments: Vec<RuntimeRequirementAssessment>,
    #[serde(default)]
    pub health_issues: Vec<HealthIssue>,
    #[serde(default)]
    pub scan_settings: ScanSettings,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HealthSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthIssue {
    pub id: String,
    pub severity: HealthSeverity,
    pub code: String,
    pub title: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manager_id: Option<PackageManagerId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogCategory {
    Scan,
    Manager,
    Project,
    Storage,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogStatus {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskLog {
    pub id: String,
    pub category: LogCategory,
    pub status: LogStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manager_id: Option<PackageManagerId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathObservation {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_path: Option<String>,
    pub alternatives: Vec<String>,
    pub has_conflict: bool,
    #[serde(default)]
    pub candidates: Vec<PathCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathCandidate {
    pub path: String,
    pub path_index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manager_id: Option<PackageManagerId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentScan {
    pub managers: Vec<PackageManager>,
    pub packages: Vec<ManagedPackage>,
    pub projects: Vec<ProjectMetadata>,
    #[serde(default)]
    pub dependency_insights: Vec<DependencyInsight>,
    #[serde(default)]
    pub workspaces: Vec<ProjectWorkspace>,
    #[serde(default)]
    pub runtime_installations: Vec<RuntimeInstallation>,
    #[serde(default)]
    pub runtime_assessments: Vec<RuntimeRequirementAssessment>,
    pub scan_roots: Vec<String>,
    #[serde(default)]
    pub scan_settings: ScanSettings,
    pub health_issues: Vec<HealthIssue>,
    pub logs: Vec<TaskLog>,
    pub path_observations: Vec<PathObservation>,
    pub scanned_at: String,
    pub partial_failures: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSummary {
    pub id: i64,
    pub scanned_at: String,
    pub manager_count: usize,
    pub package_count: usize,
    pub project_count: usize,
    pub health_issue_count: usize,
}

impl SnapshotSummary {
    pub fn from_scan(id: i64, scan: &EnvironmentScan) -> Self {
        Self {
            id,
            scanned_at: scan.scanned_at.clone(),
            manager_count: scan.managers.len(),
            package_count: scan.packages.len(),
            project_count: scan.projects.len(),
            health_issue_count: scan.health_issues.len(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotChangeKind {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotChangeEntity {
    Manager,
    Package,
    Project,
    Health,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotChange {
    pub kind: SnapshotChangeKind,
    pub entity: SnapshotChangeEntity,
    pub key: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotComparison {
    pub baseline: SnapshotSummary,
    pub current: SnapshotSummary,
    pub changes: Vec<SnapshotChange>,
    pub added_count: usize,
    pub removed_count: usize,
    pub changed_count: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanPhase {
    Managers,
    Projects,
    Runtimes,
    Health,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub scan_id: String,
    pub phase: ScanPhase,
    pub completed: usize,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manager_id: Option<PackageManagerId>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        CacheScanStatus, EnvironmentScan, ExecutionTrust, NetworkPolicy, PackageManager,
        ProjectDependency, ProjectMetadata, ScanSettings,
    };

    #[test]
    fn accepts_project_snapshots_without_dependencies() {
        let project: ProjectMetadata = serde_json::from_value(json!({
            "name": "legacy",
            "path": "/tmp/legacy",
            "ecosystems": [],
            "lockFiles": [],
            "runtimeRequirements": [],
            "warnings": []
        }))
        .unwrap();
        assert!(project.dependencies.is_empty());
    }

    #[test]
    fn accepts_environment_snapshots_without_dependency_insights() {
        let scan: EnvironmentScan = serde_json::from_value(json!({
            "managers": [], "packages": [], "projects": [], "scanRoots": [],
            "healthIssues": [], "logs": [], "pathObservations": [{
                "command": "node", "alternatives": [], "hasConflict": false
            }],
            "scannedAt": "2026-01-01T00:00:00Z", "partialFailures": 0
        }))
        .unwrap();
        assert!(scan.dependency_insights.is_empty());
        assert!(scan.path_observations[0].candidates.is_empty());
        assert_eq!(scan.scan_settings, ScanSettings::default());
        assert_eq!(scan.scan_settings.network_policy, NetworkPolicy::Offline);
        assert!(scan.runtime_installations.is_empty());
        assert!(scan.runtime_assessments.is_empty());
    }

    #[test]
    fn accepts_dependency_insights_without_resolution_fields() {
        let dependency: ProjectDependency = serde_json::from_value(json!({
            "ecosystem": "JavaScript", "name": "react", "normalizedName": "react",
            "versionRequirement": "^19", "scopes": ["运行"]
        }))
        .unwrap();
        assert!(dependency.resolved_version.is_none());
        assert!(!dependency.resolution_checked);
    }

    #[test]
    fn accepts_legacy_manager_without_execution_trust() {
        let manager: PackageManager = serde_json::from_value(json!({
            "id":"npm","displayName":"npm","status":"available","capabilities":[],"scannedAt":"now"
        }))
        .unwrap();
        assert_eq!(manager.execution_trust, ExecutionTrust::NotApplicable);
        assert_eq!(manager.cache_scan_status, CacheScanStatus::NotApplicable);
    }
}
