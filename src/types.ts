// 数据契约类型由 Rust models 生成（src/types.gen.ts）；此处只保留 UI 专属类型。
export * from "./types.gen";

import type {
  ActionCapability,
  CatalogSearchResponse,
  EnvironmentScan,
  HealthIssue,
  ManagedPackage,
  PackageAction,
  PackageActionAuditRecord,
  PackageActionPlan,
  PackageActionProgress,
  PackageActionReconciliationResult,
  PackageActionResult,
  ProjectAnalysis,
  ProjectDependencyGraph,
  ProjectMetadata,
  ProjectSupplyChainReport,
  ReportExportResult,
  ReportFormat,
  ScanProgress,
  ScanSettings,
  SnapshotComparison,
  SnapshotSummary,
  TaskLog,
} from "./types.gen";

export type PageId =
  | "overview"
  | "packages"
  | "actions"
  | "projects"
  | "analysis"
  | "runtimes"
  | "history"
  | "environment"
  | "logs"
  | "settings";

export type ProjectAnalysisView = "index" | "graph" | "supplyChain";

export type WritableManagerId = "homebrew" | "npm" | "pnpm";

export interface DevPkgApi {
  getLatestSnapshot(): Promise<EnvironmentScan | null>;
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
  exportSnapshotComparisonReport(
    format: ReportFormat,
    baselineId: number,
    currentId: number,
  ): Promise<ReportExportResult>;
  getHealthReport(): Promise<HealthIssue[]>;
  getScanLogs(): Promise<TaskLog[]>;
  getProjectDependencyGraph(projectPath: string): Promise<ProjectDependencyGraph>;
  getProjectSupplyChainReport(projectPath: string): Promise<ProjectSupplyChainReport>;
  exportProjectSbom(projectPath: string): Promise<ReportExportResult>;
  searchPackageCatalog(searchId: string, managerId: WritableManagerId, query: string): Promise<CatalogSearchResponse>;
  cancelPackageCatalogSearch(searchId: string): Promise<void>;
  planPackageAction(managerId: WritableManagerId, action: PackageAction, targets: string[]): Promise<PackageActionPlan>;
  getPackageActionCapabilities(): Promise<ActionCapability[]>;
  executePackageAction(planId: string): Promise<PackageActionResult>;
  cancelPackageAction(actionId: string): Promise<void>;
  listenToPackageActionProgress(listener: (progress: PackageActionProgress) => void): Promise<() => void>;
  listPackageActionAudit(): Promise<PackageActionAuditRecord[]>;
  reconcilePackageAction(actionId: string): Promise<PackageActionReconciliationResult>;
}
