import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { DependencyInsight, DevPkgApi, EnvironmentScan, HealthIssue, ManagedPackage, ProjectAnalysis, ProjectMetadata, ScanProgress, TaskLog } from "./types";
import { mockProjects, mockScan } from "./mock-data";

const isTauri = () => "__TAURI_INTERNALS__" in window;
let browserProjects = [...mockProjects];
let browserScanRoots = [...mockScan.scanRoots];

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
  };
};

const mockApi: DevPkgApi = {
  async scanEnvironment(scanId) {
    const total = 10;
    for (let completed = 0; completed < total; completed += 1) {
      await wait(45);
      if (cancelledMockScans.delete(scanId)) throw new Error("扫描已取消");
      emitMockProgress({
        scanId,
        phase: completed < 8 ? "managers" : completed === 8 ? "projects" : "health",
        completed,
        total,
      });
    }
    emitMockProgress({ scanId, phase: "complete", completed: total, total });
    return { ...mockScan, ...analyzeMockProjects(browserProjects), scanRoots: browserScanRoots, scannedAt: new Date().toISOString() };
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
  async getHealthReport() {
    return mockScan.healthIssues;
  },
  async getScanLogs() {
    return mockScan.logs;
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
  getHealthReport: () => invoke<HealthIssue[]>("get_health_report"),
  getScanLogs: () => invoke<TaskLog[]>("get_scan_logs"),
};

export const api: DevPkgApi = new Proxy(tauriApi, {
  get(target, property: keyof DevPkgApi) {
    return isTauri() ? target[property] : mockApi[property];
  },
});
