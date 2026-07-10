export type PageId = "overview" | "packages" | "projects" | "environment";

export type PackageManagerId = "homebrew" | "npm" | "pnpm" | "uv" | "pip";

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
  warnings: string[];
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
}

export interface EnvironmentScan {
  managers: PackageManager[];
  packages: ManagedPackage[];
  projects: ProjectMetadata[];
  scanRoots: string[];
  healthIssues: HealthIssue[];
  logs: TaskLog[];
  pathObservations: PathObservation[];
  scannedAt: string;
  partialFailures: number;
}

export interface DevPkgApi {
  scanEnvironment(): Promise<EnvironmentScan>;
  listPackages(): Promise<ManagedPackage[]>;
  listProjects(): Promise<ProjectMetadata[]>;
  addScanRoot(path: string): Promise<ProjectMetadata[]>;
  removeScanRoot(path: string): Promise<ProjectMetadata[]>;
  getHealthReport(): Promise<HealthIssue[]>;
  getScanLogs(): Promise<TaskLog[]>;
}
