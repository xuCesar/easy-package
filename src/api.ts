import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { DependencyInsight, DevPkgApi, EnvironmentScan, HealthIssue, ManagedPackage, ProjectAnalysis, ProjectDependencyGraph, ProjectMetadata, ProjectSupplyChainReport, ScanProgress, ScanSettings, SnapshotComparison, SnapshotSummary, TaskLog } from "./types";
import { mockProjects, mockScan } from "./mock-data";

export const isTauriRuntime = () => "__TAURI_INTERNALS__" in window;
let browserProjects = [...mockProjects];
let browserScanRoots = [...mockScan.scanRoots];
let browserScanSettings: ScanSettings = structuredClone(mockScan.scanSettings);

const mockSnapshotSummaries: SnapshotSummary[] = [
  { id: 2, scannedAt: nowMinusMinutes(5), managerCount: mockScan.managers.length, packageCount: mockScan.packages.length, projectCount: mockScan.projects.length, healthIssueCount: mockScan.healthIssues.length },
  { id: 1, scannedAt: nowMinusMinutes(60), managerCount: mockScan.managers.length - 1, packageCount: mockScan.packages.length - 2, projectCount: mockScan.projects.length - 1, healthIssueCount: mockScan.healthIssues.length + 1 },
];

const mockSnapshotComparison: SnapshotComparison = {
  baseline: mockSnapshotSummaries[1],
  current: mockSnapshotSummaries[0],
  addedCount: 3,
  removedCount: 1,
  changedCount: 2,
  changes: [
    { kind: "added", entity: "manager", key: "composer", title: "新增包管理器：Composer", description: "本次扫描发现该包管理器。" },
    { kind: "added", entity: "package", key: "composer:psr/log", title: "新增软件包：psr/log", description: "composer · 3.0.2" },
    { kind: "changed", entity: "package", key: "npm:typescript", title: "软件包已变化：typescript", description: "npm：5.8.3 → 5.9.3" },
    { kind: "changed", entity: "project", key: "~/Code/easy-package", title: "项目元数据已变化：easy-package", description: "生态、锁文件、运行时或直接依赖声明已变化。" },
    { kind: "removed", entity: "health", key: "legacy-warning", title: "健康提示已消失：旧版运行时", description: "该提示未出现在本次扫描结果中。" },
  ],
};

function nowMinusMinutes(minutes: number) {
  return new Date(Date.now() - minutes * 60_000).toISOString();
}

const wait = (duration = 180) => new Promise((resolve) => window.setTimeout(resolve, duration));
const mockProgressListeners = new Set<(progress: ScanProgress) => void>();
const cancelledMockScans = new Set<string>();

const emitMockProgress = (progress: ScanProgress) => {
  mockProgressListeners.forEach((listener) => listener(progress));
};

const analyzeMockProjects = (projects: ProjectMetadata[]): ProjectAnalysis => {
  const insights = new Map<string, DependencyInsight>();
  for (const project of projects) {
    for (const dependency of project.dependencies) {
      const key = `${dependency.ecosystem}:${dependency.normalizedName}`;
      const insight = insights.get(key) ?? { ecosystem: dependency.ecosystem, name: dependency.name, projectCount: 0, versionRequirements: [], resolvedVersions: [], projects: [], hasVersionDivergence: false, hasResolvedVersionDivergence: false, hasResolutionRisk: false, hasHealthRisk: false };
      insight.projectCount += 1;
      if (!insight.versionRequirements.includes(dependency.versionRequirement)) insight.versionRequirements.push(dependency.versionRequirement);
      if (dependency.resolvedVersion && !insight.resolvedVersions.includes(dependency.resolvedVersion)) insight.resolvedVersions.push(dependency.resolvedVersion);
      if (dependency.resolutionChecked && !dependency.resolvedVersion) insight.hasResolutionRisk = true;
      insight.projects.push({ projectName: project.name, projectPath: project.path, versionRequirement: dependency.versionRequirement, scopes: dependency.scopes, resolvedVersion: dependency.resolvedVersion, resolutionSource: dependency.resolutionSource });
      insights.set(key, insight);
    }
  }
  return {
    projects,
    dependencyInsights: [...insights.values()].map((insight) => ({ ...insight, versionRequirements: [...insight.versionRequirements].sort(), resolvedVersions: [...insight.resolvedVersions].sort(), hasVersionDivergence: insight.versionRequirements.length > 1, hasResolvedVersionDivergence: insight.resolvedVersions.length > 1, hasHealthRisk: insight.versionRequirements.length > 1 || insight.resolvedVersions.length > 1 || insight.hasResolutionRisk || insight.versionRequirements.some((requirement) => requirement === "未声明版本" || requirement.startsWith("workspace:") || requirement.startsWith("file:")) })),
    workspaces: mockScan.workspaces.filter((workspace) => workspace.memberPaths.some((path) => projects.some((project) => project.path === path))),
    runtimeAssessments: mockScan.runtimeAssessments.filter((assessment) => projects.some((project) => project.path === assessment.projectPath)),
    healthIssues: mockScan.healthIssues,
    scanSettings: browserScanSettings,
  };
};

const mockProjectDependencyGraph = (projectPath: string): ProjectDependencyGraph => {
  const project = browserProjects.find((item) => item.path === projectPath);
  if (!project) throw new Error("该路径不是已识别项目");
  const rootId = `project:${project.name}`;
  const supported = project.lockFiles.some((file) => ["package-lock.json", "pnpm-lock.yaml", "Cargo.lock"].includes(file));
  const packageNodes = supported ? project.dependencies.map((dependency) => ({
    id: `mock:${dependency.ecosystem}:${dependency.normalizedName}:${dependency.resolvedVersion ?? dependency.versionRequirement}`,
    ecosystem: dependency.ecosystem,
    name: dependency.name,
    version: dependency.resolvedVersion ?? dependency.versionRequirement,
    kind: "package" as const,
    direct: true,
    scopes: dependency.scopes,
  })) : [];
  const nodes = [{ id: rootId, ecosystem: "Project", name: project.name, version: "", kind: "project" as const, direct: false, scopes: [] }, ...packageNodes];
  const edges = packageNodes.map((node) => ({ from: rootId, to: node.id, dependencyType: "runtime" }));
  const completeness = supported ? "complete" as const : "unsupported" as const;
  const sources = project.lockFiles.filter((file) => ["package-lock.json", "pnpm-lock.yaml", "Cargo.lock"].includes(file));
  const summary = { nodeCount: nodes.length, edgeCount: edges.length, directCount: packageNodes.length, transitiveCount: 0, duplicateVersionCount: 0, unreachableCount: 0, cycleCount: 0, completeness, sources, sourceDigest: supported ? "mock-lock-digest" : "" };
  return { projectName: project.name, projectPath, completeness, sources, sourceDigest: summary.sourceDigest, nodes, edges, warnings: supported ? [] : ["当前项目没有受支持的完整依赖图锁文件。"], summary };
};

const mockProjectSupplyChainReport = (projectPath: string): ProjectSupplyChainReport => {
  const graph = mockProjectDependencyGraph(projectPath);
  const project = browserProjects.find((item) => item.path === projectPath)!;
  const findings = graph.completeness === "unsupported" && project.dependencies.length ? [{
    id: `${projectPath}:LOCKFILE_MISSING`, code: "LOCKFILE_MISSING", severity: "warning" as const,
    title: "缺少受支持的完整依赖锁文件", description: "项目声明了依赖，但无法从 npm、pnpm 或 Cargo 锁文件重建完整依赖图。",
    projectPath, dependencyPath: [], evidence: project.lockFiles,
  }] : graph.nodes.filter((node) => node.kind !== "project" && !node.packageUrl).map((node) => ({
    id: `${projectPath}:PACKAGE_SOURCE_UNKNOWN:${node.id}`, code: "PACKAGE_SOURCE_UNKNOWN", severity: "info" as const,
    title: "依赖来源无法规范化", description: "锁文件没有提供足够信息来生成 Package URL；这不等同于已确认存在风险。",
    projectPath, nodeId: node.id, dependencyPath: [project.name, `${node.name}@${node.version}`], evidence: [`${node.name} ${node.version}`],
  }));
  const warningCount = findings.filter((finding) => finding.severity === "warning").length;
  return {
    projectName: project.name,
    projectPath,
    summary: { totalCount: findings.length, warningCount, infoCount: findings.length - warningCount, ruleIds: [...new Set(findings.map((finding) => finding.code))].sort() },
    findings,
  };
};

const mockApi: DevPkgApi = {
  async scanEnvironment(scanId) {
    const total = 13;
    for (let completed = 0; completed < total; completed += 1) {
      await wait(45);
      if (cancelledMockScans.delete(scanId)) throw new Error("扫描已取消");
      emitMockProgress({
        scanId,
        phase: completed < 10 ? "managers" : completed === 10 ? "projects" : completed === 11 ? "runtimes" : "health",
        completed,
        total,
      });
    }
    emitMockProgress({ scanId, phase: "complete", completed: total, total });
    return { ...mockScan, ...analyzeMockProjects(browserProjects), scanRoots: browserScanRoots, scanSettings: browserScanSettings, scannedAt: new Date().toISOString() };
  },
  async cancelEnvironmentScan(scanId) {
    cancelledMockScans.add(scanId);
  },
  async listenToScanProgress(listener) {
    mockProgressListeners.add(listener);
    return () => mockProgressListeners.delete(listener);
  },
  async listPackages() {
    return mockScan.packages;
  },
  async listProjects() {
    return browserProjects;
  },
  async addScanRoot(path) {
    const name = path.split("/").filter(Boolean).at(-1) ?? "project";
    if (!browserProjects.some((project) => project.path === path)) {
      browserProjects = [...browserProjects, { name, path, ecosystems: [], lockFiles: [], runtimeRequirements: [], dependencies: [], warnings: ["浏览器预览模式未读取本机文件"] }];
    }
    if (!browserScanRoots.includes(path)) browserScanRoots = [...browserScanRoots, path];
    return analyzeMockProjects(browserProjects);
  },
  async removeScanRoot(path) {
    browserProjects = browserProjects.filter((project) => project.path !== path && !project.path.startsWith(`${path}/`));
    browserScanRoots = browserScanRoots.filter((root) => root !== path);
    return analyzeMockProjects(browserProjects);
  },
  async getScanSettings() {
    return browserScanSettings;
  },
  async updateScanSettings(settings) {
    if (!Number.isInteger(settings.maxDepth) || settings.maxDepth < 1 || settings.maxDepth > 12) throw new Error("扫描范围设置无效：最大扫描深度需在 1 到 12 之间");
    browserScanSettings = { ...settings, ignoredPaths: [...new Set(settings.ignoredPaths)].sort(), defaultIgnoredDirectoryNames: [...mockScan.scanSettings.defaultIgnoredDirectoryNames] };
    return analyzeMockProjects(browserProjects);
  },
  async exportEnvironmentReport() {
    return { saved: true };
  },
  async listSnapshotSummaries() {
    return mockSnapshotSummaries;
  },
  async compareSnapshots(baselineId, currentId) {
    if (baselineId === currentId) throw new Error("请选择两个不同的快照进行比较");
    return { ...mockSnapshotComparison, baseline: mockSnapshotSummaries.find((summary) => summary.id === baselineId) ?? mockSnapshotComparison.baseline, current: mockSnapshotSummaries.find((summary) => summary.id === currentId) ?? mockSnapshotComparison.current };
  },
  async exportSnapshotComparisonReport() {
    return { saved: true };
  },
  async getHealthReport() {
    return mockScan.healthIssues;
  },
  async getScanLogs() {
    return mockScan.logs;
  },
  async getProjectDependencyGraph(projectPath) {
    return mockProjectDependencyGraph(projectPath);
  },
  async getProjectSupplyChainReport(projectPath) {
    return mockProjectSupplyChainReport(projectPath);
  },
  async exportProjectSbom() {
    return { saved: true };
  },
};

const tauriApi: DevPkgApi = {
  scanEnvironment: (scanId) => invoke<EnvironmentScan>("scan_environment", { scanId }),
  cancelEnvironmentScan: (scanId) => invoke<void>("cancel_environment_scan", { scanId }),
  async listenToScanProgress(listener) {
    return listen<ScanProgress>("scan-progress", (event) => listener(event.payload));
  },
  listPackages: () => invoke<ManagedPackage[]>("list_packages"),
  listProjects: () => invoke<ProjectMetadata[]>("list_projects"),
  addScanRoot: (path) => invoke<ProjectAnalysis>("add_scan_root", { path }),
  removeScanRoot: (path) => invoke<ProjectAnalysis>("remove_scan_root", { path }),
  getScanSettings: () => invoke<ScanSettings>("get_scan_settings"),
  updateScanSettings: (settings) => invoke<ProjectAnalysis>("update_scan_settings", { settings }),
  exportEnvironmentReport: (format) => invoke("export_environment_report", { format }),
  listSnapshotSummaries: () => invoke<SnapshotSummary[]>("list_snapshot_summaries"),
  compareSnapshots: (baselineId, currentId) => invoke<SnapshotComparison>("compare_snapshots", { baselineId, currentId }),
  exportSnapshotComparisonReport: (format, baselineId, currentId) => invoke("export_snapshot_comparison_report", { format, baselineId, currentId }),
  getHealthReport: () => invoke<HealthIssue[]>("get_health_report"),
  getScanLogs: () => invoke<TaskLog[]>("get_scan_logs"),
  getProjectDependencyGraph: (projectPath) => invoke<ProjectDependencyGraph>("get_project_dependency_graph", { projectPath }),
  getProjectSupplyChainReport: (projectPath) => invoke<ProjectSupplyChainReport>("get_project_supply_chain_report", { projectPath }),
  exportProjectSbom: (projectPath) => invoke("export_project_sbom", { projectPath }),
};

export const api: DevPkgApi = new Proxy(tauriApi, {
  get(target, property: keyof DevPkgApi) {
    return isTauriRuntime() ? target[property] : mockApi[property];
  },
});
