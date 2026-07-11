export type PageId = "overview" | "packages" | "projects" | "dependencies" | "environment";

export type PackageManagerId = "homebrew" | "npm" | "pnpm" | "uv" | "pip" | "yarn" | "bun" | "cargo" | "rubygems" | "composer";

export type ManagerStatus = "available" | "unavailable" | "error" | "unsupported";

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
  capabilities: string[];
  error?: DiagnosticError;
  cacheSizeBytes?: number;
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
  scanSettings: ScanSettings;
}

export interface ScanSettings {
  ignoredPaths: string[];
  maxDepth: number;
  defaultIgnoredDirectoryNames: string[];
}

export type ReportFormat = "json" | "markdown";

export interface ReportExportResult {
  saved: boolean;
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
  scanRoots: string[];
  scanSettings: ScanSettings;
  healthIssues: HealthIssue[];
  logs: TaskLog[];
  pathObservations: PathObservation[];
  scannedAt: string;
  partialFailures: number;
}

export type ScanPhase = "managers" | "projects" | "health" | "complete";

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
  getHealthReport(): Promise<HealthIssue[]>;
  getScanLogs(): Promise<TaskLog[]>;
}
