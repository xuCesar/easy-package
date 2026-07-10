import { invoke } from "@tauri-apps/api/core";
import type { DevPkgApi, EnvironmentScan, HealthIssue, ManagedPackage, ProjectMetadata, TaskLog } from "./types";
import { mockProjects, mockScan } from "./mock-data";

const isTauri = () => "__TAURI_INTERNALS__" in window;
let browserProjects = [...mockProjects];
let browserScanRoots = [...mockScan.scanRoots];

const wait = (duration = 180) => new Promise((resolve) => window.setTimeout(resolve, duration));

const mockApi: DevPkgApi = {
  async scanEnvironment() {
    await wait();
    return { ...mockScan, projects: browserProjects, scanRoots: browserScanRoots, scannedAt: new Date().toISOString() };
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
      browserProjects = [...browserProjects, { name, path, ecosystems: [], lockFiles: [], runtimeRequirements: [], warnings: ["浏览器预览模式未读取本机文件"] }];
    }
    if (!browserScanRoots.includes(path)) browserScanRoots = [...browserScanRoots, path];
    return browserProjects;
  },
  async removeScanRoot(path) {
    browserProjects = browserProjects.filter((project) => project.path !== path && !project.path.startsWith(`${path}/`));
    browserScanRoots = browserScanRoots.filter((root) => root !== path);
    return browserProjects;
  },
  async getHealthReport() {
    return mockScan.healthIssues;
  },
  async getScanLogs() {
    return mockScan.logs;
  },
};

const tauriApi: DevPkgApi = {
  scanEnvironment: () => invoke<EnvironmentScan>("scan_environment"),
  listPackages: () => invoke<ManagedPackage[]>("list_packages"),
  listProjects: () => invoke<ProjectMetadata[]>("list_projects"),
  addScanRoot: (path) => invoke<ProjectMetadata[]>("add_scan_root", { path }),
  removeScanRoot: (path) => invoke<ProjectMetadata[]>("remove_scan_root", { path }),
  getHealthReport: () => invoke<HealthIssue[]>("get_health_report"),
  getScanLogs: () => invoke<TaskLog[]>("get_scan_logs"),
};

export const api: DevPkgApi = new Proxy(tauriApi, {
  get(target, property: keyof DevPkgApi) {
    return isTauri() ? target[property] : mockApi[property];
  },
});
