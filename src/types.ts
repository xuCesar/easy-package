export type PageId = "overview" | "packages" | "actions" | "projects" | "dependencies" | "supplyChain" | "runtimes" | "history" | "environment";

export type PackageManagerId = "homebrew" | "npm" | "pnpm" | "uv" | "pip" | "yarn" | "bun" | "cargo" | "rubygems" | "composer";

export type ManagerStatus = "available" | "unavailable" | "error" | "blocked" | "unsupported";
export type ExecutionTrust = "system" | "managed" | "userManaged" | "unverified" | "notApplicable";
export type CacheScanStatus = "complete" | "partial" | "unavailable" | "notApplicable";

export interface DiagnosticError {
  code: string;
  message: string;
  exitCode?: number;
  output?: string;
}

export interface PackageManager {
  id: PackageManagerId;
  displayName: string;
  version?: string;
  executablePath?: string;
  status: ManagerStatus;
  executionTrust: ExecutionTrust;
  capabilities: string[];
  error?: DiagnosticError;
  cacheSizeBytes?: number;
  cacheScanStatus: CacheScanStatus;
  scannedAt: string;
}

export type PackageScope = "system" | "global" | "tool";
export type UpdateStatus = "upToDate" | "available" | "unknown";

export interface ManagedPackage {
  id: string;
  managerId: PackageManagerId;
  name: string;
  version: string;
  latestVersion?: string;
  scope: PackageScope;
  updateStatus: UpdateStatus;
}

export interface RuntimeRequirement {
  runtime: string;
  requirement: string;
}

export interface ProjectMetadata {
  name: string;
  path: string;
  ecosystems: string[];
  lockFiles: string[];
  runtimeRequirements: RuntimeRequirement[];
  packageManager?: string;
  dependencies: ProjectDependency[];
  workspace?: ProjectWorkspaceRef;
  dependencyGraphSummary?: DependencyGraphSummary;
  supplyChainRiskSummary?: SupplyChainRiskSummary;
  warnings: string[];
}

export interface ProjectWorkspaceRef {
  name: string;
  path: string;
  ecosystem: string;
}

export interface ProjectWorkspace {
  name: string;
  path: string;
  ecosystem: string;
  memberPaths: string[];
}

export interface ProjectDependency {
  ecosystem: string;
  name: string;
  normalizedName: string;
  versionRequirement: string;
  scopes: string[];
  resolvedVersion?: string;
  resolutionSource?: string;
  resolutionChecked: boolean;
}

export type DependencyGraphCompleteness = "complete" | "partial" | "unsupported" | "invalid";
export type DependencyGraphNodeKind = "project" | "package" | "local";

export interface DependencyGraphNode {
  id: string;
  ecosystem: string;
  name: string;
  version: string;
  kind: DependencyGraphNodeKind;
  direct: boolean;
  scopes: string[];
  packageUrl?: string;
}

export interface DependencyGraphEdge {
  from: string;
  to: string;
  dependencyType: string;
}

export interface DependencyGraphSummary {
  nodeCount: number;
  edgeCount: number;
  directCount: number;
  transitiveCount: number;
  duplicateVersionCount: number;
  unreachableCount: number;
  cycleCount: number;
  completeness: DependencyGraphCompleteness;
  sources: string[];
  sourceDigest: string;
}

export interface ProjectDependencyGraph {
  projectName: string;
  projectPath: string;
  completeness: DependencyGraphCompleteness;
  sources: string[];
  sourceDigest: string;
  nodes: DependencyGraphNode[];
  edges: DependencyGraphEdge[];
  warnings: string[];
  summary: DependencyGraphSummary;
}

export type SupplyChainRiskSeverity = "info" | "warning";

export interface SupplyChainRiskFinding {
  id: string;
  code: string;
  severity: SupplyChainRiskSeverity;
  title: string;
  description: string;
  projectPath: string;
  nodeId?: string;
  dependencyPath: string[];
  evidence: string[];
}

export interface SupplyChainRiskSummary {
  totalCount: number;
  warningCount: number;
  infoCount: number;
  ruleIds: string[];
}

export interface ProjectSupplyChainReport {
  projectName: string;
  projectPath: string;
  summary: SupplyChainRiskSummary;
  findings: SupplyChainRiskFinding[];
}

export interface DependencyProjectUsage {
  projectName: string;
  projectPath: string;
  versionRequirement: string;
  scopes: string[];
  resolvedVersion?: string;
  resolutionSource?: string;
}

export interface DependencyInsight {
  ecosystem: string;
  name: string;
  projectCount: number;
  versionRequirements: string[];
  resolvedVersions: string[];
  projects: DependencyProjectUsage[];
  hasVersionDivergence: boolean;
  hasResolvedVersionDivergence: boolean;
  hasResolutionRisk: boolean;
  hasHealthRisk: boolean;
}

export interface ProjectAnalysis {
  projects: ProjectMetadata[];
  dependencyInsights: DependencyInsight[];
  workspaces: ProjectWorkspace[];
  runtimeAssessments: RuntimeRequirementAssessment[];
  healthIssues: HealthIssue[];
  scanSettings: ScanSettings;
}

export interface ScanSettings {
  ignoredPaths: string[];
  maxDepth: number;
  defaultIgnoredDirectoryNames: string[];
  networkPolicy: "offline" | "registry";
}

export interface RuntimeInstallation {
  id: string;
  runtime: string;
  version: string;
  path: string;
  provider: string;
  isActive: boolean;
  executionTrust: ExecutionTrust;
}

export type RuntimeRequirementStatus = "available" | "missing" | "mismatch" | "unknown";

export interface RuntimeRequirementAssessment {
  projectName: string;
  projectPath: string;
  runtime: string;
  requirement: string;
  status: RuntimeRequirementStatus;
  activeVersion?: string;
  installedVersions: string[];
  message: string;
}

export type ReportFormat = "json" | "markdown";

export interface ReportExportResult {
  saved: boolean;
}

export interface SnapshotSummary {
  id: number;
  scannedAt: string;
  managerCount: number;
  packageCount: number;
  projectCount: number;
  healthIssueCount: number;
}

export type SnapshotChangeKind = "added" | "removed" | "changed";
export type SnapshotChangeEntity = "manager" | "package" | "project" | "health";

export interface SnapshotChange {
  kind: SnapshotChangeKind;
  entity: SnapshotChangeEntity;
  key: string;
  title: string;
  description: string;
}

export interface SnapshotComparison {
  baseline: SnapshotSummary;
  current: SnapshotSummary;
  changes: SnapshotChange[];
  addedCount: number;
  removedCount: number;
  changedCount: number;
}

export type PackageAction = "install" | "upgrade" | "uninstall" | "cleanup";
export type PackageActionStatus = "planned" | "running" | "succeeded" | "failed" | "unknown";

export interface PackageActionPlan {
  id: string;
  managerId: "homebrew";
  action: PackageAction;
  targets: string[];
  commandPreview: string;
  warnings: string[];
  previewLines: string[];
  requiresNetwork: boolean;
  createdAt: string;
}

export interface PackageActionProgress {
  actionId: string;
  status: PackageActionStatus;
  message: string;
  cancellable: boolean;
  timestamp: string;
}

export interface PackageActionResult {
  actionId: string;
  planId: string;
  managerId: "homebrew";
  action: PackageAction;
  targets: string[];
  status: PackageActionStatus;
  commandPreview: string;
  logs: string[];
  error?: string;
  comparison?: SnapshotComparison;
  environment?: EnvironmentScan;
  startedAt: string;
  finishedAt: string;
}

export interface PackageActionAuditRecord {
  actionId: string;
  planId: string;
  managerId: "homebrew";
  action: PackageAction;
  targets: string[];
  status: PackageActionStatus;
  commandPreview: string;
  logs: string[];
  error?: string;
  startedAt: string;
  finishedAt: string;
}

export type HealthSeverity = "info" | "warning" | "error";

export interface HealthIssue {
  id: string;
  severity: HealthSeverity;
  code: string;
  title: string;
  description: string;
  managerId?: PackageManagerId;
  path?: string;
  command?: string;
}

export interface TaskLog {
  id: string;
  category: "scan" | "manager" | "project" | "storage";
  status: "info" | "success" | "warning" | "error";
  message: string;
  managerId?: PackageManagerId;
  exitCode?: number;
  output?: string;
  timestamp: string;
}

export interface PathObservation {
  command: string;
  activePath?: string;
  alternatives: string[];
  hasConflict: boolean;
  candidates?: PathCandidate[];
}

export interface PathCandidate {
  path: string;
  pathIndex: number;
  managerId?: PackageManagerId;
  version?: string;
}

export interface EnvironmentScan {
  managers: PackageManager[];
  packages: ManagedPackage[];
  projects: ProjectMetadata[];
  dependencyInsights: DependencyInsight[];
  workspaces: ProjectWorkspace[];
  runtimeInstallations: RuntimeInstallation[];
  runtimeAssessments: RuntimeRequirementAssessment[];
  scanRoots: string[];
  scanSettings: ScanSettings;
  healthIssues: HealthIssue[];
  logs: TaskLog[];
  pathObservations: PathObservation[];
  scannedAt: string;
  partialFailures: number;
}

export type ScanPhase = "managers" | "projects" | "runtimes" | "health" | "complete";

export interface ScanProgress {
  scanId: string;
  phase: ScanPhase;
  completed: number;
  total: number;
  managerId?: PackageManagerId;
}

export interface DevPkgApi {
  scanEnvironment(scanId: string): Promise<EnvironmentScan>;
  cancelEnvironmentScan(scanId: string): Promise<void>;
  listenToScanProgress(listener: (progress: ScanProgress) => void): Promise<() => void>;
  listPackages(): Promise<ManagedPackage[]>;
  listProjects(): Promise<ProjectMetadata[]>;
  addScanRoot(path: string): Promise<ProjectAnalysis>;
  removeScanRoot(path: string): Promise<ProjectAnalysis>;
  getScanSettings(): Promise<ScanSettings>;
  updateScanSettings(settings: ScanSettings): Promise<ProjectAnalysis>;
  exportEnvironmentReport(format: ReportFormat): Promise<ReportExportResult>;
  listSnapshotSummaries(): Promise<SnapshotSummary[]>;
  compareSnapshots(baselineId: number, currentId: number): Promise<SnapshotComparison>;
  exportSnapshotComparisonReport(format: ReportFormat, baselineId: number, currentId: number): Promise<ReportExportResult>;
  getHealthReport(): Promise<HealthIssue[]>;
  getScanLogs(): Promise<TaskLog[]>;
  getProjectDependencyGraph(projectPath: string): Promise<ProjectDependencyGraph>;
  getProjectSupplyChainReport(projectPath: string): Promise<ProjectSupplyChainReport>;
  exportProjectSbom(projectPath: string): Promise<ReportExportResult>;
  planHomebrewAction(action: PackageAction, targets: string[]): Promise<PackageActionPlan>;
  executePackageAction(planId: string): Promise<PackageActionResult>;
  cancelPackageAction(actionId: string): Promise<void>;
  listenToPackageActionProgress(listener: (progress: PackageActionProgress) => void): Promise<() => void>;
  listPackageActionAudit(): Promise<PackageActionAuditRecord[]>;
}
