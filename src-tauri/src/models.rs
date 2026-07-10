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
    Unsupported,
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
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<DiagnosticError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_size_bytes: Option<u64>,
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
    pub scan_roots: Vec<String>,
    pub health_issues: Vec<HealthIssue>,
    pub logs: Vec<TaskLog>,
    pub path_observations: Vec<PathObservation>,
    pub scanned_at: String,
    pub partial_failures: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanPhase {
    Managers,
    Projects,
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

    use super::{EnvironmentScan, ProjectDependency, ProjectMetadata};

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
            "healthIssues": [], "logs": [], "pathObservations": [],
            "scannedAt": "2026-01-01T00:00:00Z", "partialFailures": 0
        }))
        .unwrap();
        assert!(scan.dependency_insights.is_empty());
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
}
