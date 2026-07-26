import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ApiError } from "./lib/apiError";
import { mockProjects, mockScan } from "./mock-data";
import type {
  ActionCapability,
  CatalogSearchResponse,
  DependencyInsight,
  DevPkgApi,
  EnvironmentScan,
  HealthIssue,
  ManagedPackage,
  PackageAction,
  PackageActionAuditRecord,
  PackageActionPlan,
  PackageActionProgress,
  PackageActionResult,
  ProjectAnalysis,
  ProjectDependencyGraph,
  ProjectMetadata,
  ProjectSupplyChainReport,
  ScanProgress,
  ScanSettings,
  SnapshotComparison,
  SnapshotSummary,
  TaskLog,
  WritableManagerId,
} from "./types";

export const isTauriRuntime = () => "__TAURI_INTERNALS__" in window;
let browserProjects = [...mockProjects];
let browserScanRoots = [...mockScan.scanRoots];
let browserScanSettings: ScanSettings = structuredClone(mockScan.scanSettings);

const mockSnapshotSummaries: SnapshotSummary[] = [
  {
    id: 2,
    scannedAt: nowMinusMinutes(5),
    managerCount: mockScan.managers.length,
    packageCount: mockScan.packages.length,
    projectCount: mockScan.projects.length,
    healthIssueCount: mockScan.healthIssues.length,
  },
  {
    id: 1,
    scannedAt: nowMinusMinutes(60),
    managerCount: mockScan.managers.length - 1,
    packageCount: mockScan.packages.length - 2,
    projectCount: mockScan.projects.length - 1,
    healthIssueCount: mockScan.healthIssues.length + 1,
  },
];

const mockSnapshotComparison: SnapshotComparison = {
  baseline: mockSnapshotSummaries[1],
  current: mockSnapshotSummaries[0],
  addedCount: 3,
  removedCount: 1,
  changedCount: 2,
  changes: [
    {
      kind: "added",
      entity: "manager",
      key: "composer",
      title: "新增包管理器：Composer",
      description: "本次扫描发现该包管理器。",
    },
    {
      kind: "added",
      entity: "package",
      key: "composer:psr/log",
      title: "新增软件包：psr/log",
      description: "composer · 3.0.2",
    },
    {
      kind: "changed",
      entity: "package",
      key: "npm:typescript",
      title: "软件包已变化：typescript",
      description: "npm：5.8.3 → 5.9.3",
    },
    {
      kind: "changed",
      entity: "project",
      key: "~/Code/easy-package",
      title: "项目元数据已变化：easy-package",
      description: "生态、锁文件、运行时或直接依赖声明已变化。",
    },
    {
      kind: "removed",
      entity: "health",
      key: "legacy-warning",
      title: "健康提示已消失：旧版运行时",
      description: "该提示未出现在本次扫描结果中。",
    },
  ],
};

function nowMinusMinutes(minutes: number) {
  return new Date(Date.now() - minutes * 60_000).toISOString();
}

const wait = (duration = 180) => new Promise((resolve) => window.setTimeout(resolve, duration));
const mockProgressListeners = new Set<(progress: ScanProgress) => void>();
const cancelledMockScans = new Set<string>();
const mockActionProgressListeners = new Set<(progress: PackageActionProgress) => void>();
const cancelledMockActions = new Set<string>();
const cancelledMockCatalogSearches = new Set<string>();
const mockActionAudit: PackageActionAuditRecord[] = [];
const mockActionPlans = new Map<string, PackageActionPlan>();

const emitMockProgress = (progress: ScanProgress) => {
  for (const listener of mockProgressListeners) {
    listener(progress);
  }
};

const emitMockActionProgress = (progress: PackageActionProgress) => {
  for (const listener of mockActionProgressListeners) {
    listener(progress);
  }
};

const actionLabel: Record<PackageAction, string> = {
  install: "install",
  upgrade: "upgrade",
  uninstall: "uninstall",
  cleanup: "cleanup",
};

const createMockActionPlan = (
  managerId: WritableManagerId,
  action: PackageAction,
  targets: string[],
): PackageActionPlan => {
  const id = crypto.randomUUID();
  const managerName = managerId === "homebrew" ? "Homebrew" : managerId;
  const warnings = [
    `该操作会修改本机 ${managerName} 环境，无法保证自动回滚。`,
    "操作期间请勿退出应用；若应用异常退出，请重新启动并刷新扫描确认实际状态。",
  ];
  if (managerId !== "homebrew") warnings.push("固定使用 --ignore-scripts，不运行软件包 lifecycle scripts。");
  if (action === "cleanup")
    warnings.push(
      managerId === "homebrew"
        ? "缓存清理会删除 Homebrew 判定为可安全移除的旧下载和版本。"
        : managerId === "pnpm"
          ? "pnpm store 可能被多个项目共享；清理后可能需要重新下载依赖。"
          : "npm cache verify 会校验并回收缓存，但不会强制清空。",
    );
  const executable =
    managerId === "homebrew"
      ? "/opt/homebrew/bin/brew"
      : managerId === "pnpm"
        ? "~/.local/share/pnpm/pnpm"
        : "/opt/homebrew/bin/npm";
  const args =
    managerId === "homebrew"
      ? `${actionLabel[action]}${targets.length ? ` ${targets.join(" ")}` : ""}`
      : managerId === "pnpm"
        ? action === "cleanup"
          ? "store prune"
          : `${action === "install" ? "add" : action === "upgrade" ? "update" : "remove"} --global --ignore-scripts ${targets.join(" ")}`
        : action === "cleanup"
          ? "cache verify"
          : `${action === "install" ? "install" : action === "upgrade" ? "update" : "uninstall"} --global --ignore-scripts ${targets.join(" ")}`;
  const previewLines =
    managerId === "npm"
      ? [
          "关联 Node.js：/opt/homebrew/bin/node",
          "全局 prefix：/opt/homebrew",
          "缓存目录：~/.npm",
          action === "cleanup"
            ? "仅执行 npm cache verify：校验缓存索引并回收无用内容，不强制清空缓存。"
            : `包名已通过严格校验：${targets[0] ?? "已扫描目标"}；固定禁用 lifecycle scripts。`,
        ]
      : managerId === "pnpm" && action === "cleanup"
        ? ["共享 store：~/.local/share/pnpm/store", "当前扫描缓存大小：3840000000 bytes"]
        : action === "cleanup"
          ? ["Would remove: ~/Library/Caches/Homebrew/downloads/example.tar.gz"]
          : action === "uninstall"
            ? [managerId === "homebrew" ? "未发现依赖该 Formula 的已安装包。" : `将移除 pnpm 全局包 ${targets[0]}。`]
            : action === "install"
              ? [
                  managerId === "homebrew"
                    ? `Formula 名称已通过严格语法校验：${targets[0]}；存在性由 Homebrew 执行时验证。`
                    : `包名已通过严格校验：${targets[0]}；固定禁用 lifecycle scripts。`,
                ]
              : [`待升级 ${managerName} 软件包均来自当前扫描结果。`];
  const plan: PackageActionPlan = {
    id,
    managerId,
    action,
    targets,
    commandPreview: `${executable} ${args}`,
    warnings,
    previewLines,
    checks: [{ code: "READY", status: "pass", title: "基础条件已满足", detail: "浏览器预览使用固定能力数据。" }],
    requiresNetwork: action === "install" || action === "upgrade",
    createdAt: new Date().toISOString(),
  };
  mockActionPlans.set(id, plan);
  return plan;
};

const mockActionComparison = (plan: PackageActionPlan): SnapshotComparison => ({
  baseline: {
    id: 2,
    scannedAt: nowMinusMinutes(5),
    managerCount: 10,
    packageCount: 11,
    projectCount: mockProjects.length,
    healthIssueCount: 2,
  },
  current: {
    id: 3,
    scannedAt: new Date().toISOString(),
    managerCount: 10,
    packageCount: plan.action === "install" ? 12 : plan.action === "uninstall" ? 10 : 11,
    projectCount: mockProjects.length,
    healthIssueCount: 2,
  },
  addedCount: plan.action === "install" ? 1 : 0,
  removedCount: plan.action === "uninstall" ? 1 : 0,
  changedCount: plan.action === "upgrade" || plan.action === "cleanup" ? 1 : 0,
  changes: [
    {
      kind: plan.action === "install" ? "added" : plan.action === "uninstall" ? "removed" : "changed",
      entity: plan.action === "cleanup" ? "manager" : "package",
      key: `${plan.managerId}:${plan.targets[0] ?? "cache"}`,
      title:
        plan.action === "cleanup"
          ? `${plan.managerId === "homebrew" ? "Homebrew" : plan.managerId} 缓存信息已变化`
          : `${plan.targets[0]} 已${plan.action === "install" ? "安装" : plan.action === "uninstall" ? "移除" : "升级"}`,
      description: "浏览器预览使用固定结果，不会修改本机环境。",
    },
  ],
});

const analyzeMockProjects = (projects: ProjectMetadata[]): ProjectAnalysis => {
  const insights = new Map<string, DependencyInsight>();
  for (const project of projects) {
    for (const dependency of project.dependencies) {
      const key = `${dependency.ecosystem}:${dependency.normalizedName}`;
      const insight = insights.get(key) ?? {
        ecosystem: dependency.ecosystem,
        name: dependency.name,
        projectCount: 0,
        versionRequirements: [],
        resolvedVersions: [],
        projects: [],
        hasVersionDivergence: false,
        hasResolvedVersionDivergence: false,
        hasResolutionRisk: false,
        hasHealthRisk: false,
      };
      insight.projectCount += 1;
      if (!insight.versionRequirements.includes(dependency.versionRequirement))
        insight.versionRequirements.push(dependency.versionRequirement);
      if (dependency.resolvedVersion && !insight.resolvedVersions.includes(dependency.resolvedVersion))
        insight.resolvedVersions.push(dependency.resolvedVersion);
      if (dependency.resolutionChecked && !dependency.resolvedVersion) insight.hasResolutionRisk = true;
      insight.projects.push({
        projectName: project.name,
        projectPath: project.path,
        versionRequirement: dependency.versionRequirement,
        scopes: dependency.scopes,
        resolvedVersion: dependency.resolvedVersion,
        resolutionSource: dependency.resolutionSource,
      });
      insights.set(key, insight);
    }
  }
  return {
    projects,
    dependencyInsights: [...insights.values()].map((insight) => ({
      ...insight,
      versionRequirements: [...insight.versionRequirements].sort(),
      resolvedVersions: [...insight.resolvedVersions].sort(),
      hasVersionDivergence: insight.versionRequirements.length > 1,
      hasResolvedVersionDivergence: insight.resolvedVersions.length > 1,
      hasHealthRisk:
        insight.versionRequirements.length > 1 ||
        insight.resolvedVersions.length > 1 ||
        insight.hasResolutionRisk ||
        insight.versionRequirements.some(
          (requirement) =>
            requirement === "未声明版本" || requirement.startsWith("workspace:") || requirement.startsWith("file:"),
        ),
    })),
    workspaces: mockScan.workspaces.filter((workspace) =>
      workspace.memberPaths.some((path) => projects.some((project) => project.path === path)),
    ),
    runtimeAssessments: mockScan.runtimeAssessments.filter((assessment) =>
      projects.some((project) => project.path === assessment.projectPath),
    ),
    healthIssues: mockScan.healthIssues,
    scanSettings: browserScanSettings,
  };
};

const mockProjectDependencyGraph = (projectPath: string): ProjectDependencyGraph => {
  const project = browserProjects.find((item) => item.path === projectPath);
  if (!project) throw new Error("该路径不是已识别项目");
  const rootId = `project:${project.name}`;
  const supported = project.lockFiles.some((file) =>
    ["package-lock.json", "pnpm-lock.yaml", "Cargo.lock"].includes(file),
  );
  const packageNodes = supported
    ? project.dependencies.map((dependency) => ({
        id: `mock:${dependency.ecosystem}:${dependency.normalizedName}:${dependency.resolvedVersion ?? dependency.versionRequirement}`,
        ecosystem: dependency.ecosystem,
        name: dependency.name,
        version: dependency.resolvedVersion ?? dependency.versionRequirement,
        kind: "package" as const,
        direct: true,
        scopes: dependency.scopes,
      }))
    : [];
  const nodes = [
    {
      id: rootId,
      ecosystem: "Project",
      name: project.name,
      version: "",
      kind: "project" as const,
      direct: false,
      scopes: [],
    },
    ...packageNodes,
  ];
  const edges = packageNodes.map((node) => ({ from: rootId, to: node.id, dependencyType: "runtime" }));
  const completeness = supported ? ("complete" as const) : ("unsupported" as const);
  const sources = project.lockFiles.filter((file) =>
    ["package-lock.json", "pnpm-lock.yaml", "Cargo.lock"].includes(file),
  );
  const summary = {
    nodeCount: nodes.length,
    edgeCount: edges.length,
    directCount: packageNodes.length,
    transitiveCount: 0,
    duplicateVersionCount: 0,
    unreachableCount: 0,
    cycleCount: 0,
    completeness,
    sources,
    sourceDigest: supported ? "mock-lock-digest" : "",
  };
  return {
    projectName: project.name,
    projectPath,
    completeness,
    sources,
    sourceDigest: summary.sourceDigest,
    nodes,
    edges,
    warnings: supported ? [] : ["当前项目没有受支持的完整依赖图锁文件。"],
    summary,
  };
};

const mockProjectSupplyChainReport = (projectPath: string): ProjectSupplyChainReport => {
  const graph = mockProjectDependencyGraph(projectPath);
  const project = browserProjects.find((item) => item.path === projectPath);
  if (!project) {
    throw new Error(`未找到项目：${projectPath}`);
  }
  const findings =
    graph.completeness === "unsupported" && project.dependencies.length
      ? [
          {
            id: `${projectPath}:LOCKFILE_MISSING`,
            code: "LOCKFILE_MISSING",
            severity: "warning" as const,
            title: "缺少受支持的完整依赖锁文件",
            description: "项目声明了依赖，但无法从 npm、pnpm 或 Cargo 锁文件重建完整依赖图。",
            projectPath,
            dependencyPath: [],
            evidence: project.lockFiles,
          },
        ]
      : graph.nodes
          .filter((node) => node.kind !== "project" && !node.packageUrl)
          .map((node) => ({
            id: `${projectPath}:PACKAGE_SOURCE_UNKNOWN:${node.id}`,
            code: "PACKAGE_SOURCE_UNKNOWN",
            severity: "info" as const,
            title: "依赖来源无法规范化",
            description: "锁文件没有提供足够信息来生成 Package URL；这不等同于已确认存在风险。",
            projectPath,
            nodeId: node.id,
            dependencyPath: [project.name, `${node.name}@${node.version}`],
            evidence: [`${node.name} ${node.version}`],
          }));
  const warningCount = findings.filter((finding) => finding.severity === "warning").length;
  return {
    projectName: project.name,
    projectPath,
    summary: {
      totalCount: findings.length,
      warningCount,
      infoCount: findings.length - warningCount,
      ruleIds: [...new Set(findings.map((finding) => finding.code))].sort(),
    },
    findings,
  };
};

const mockApi: DevPkgApi = {
  async scanEnvironment(scanId) {
    const total = 13;
    for (let completed = 0; completed < total; completed += 1) {
      await wait(45);
      if (cancelledMockScans.delete(scanId)) throw new ApiError({ code: "SCAN_CANCELLED", message: "扫描已取消" });
      emitMockProgress({
        scanId,
        phase: completed < 10 ? "managers" : completed === 10 ? "projects" : completed === 11 ? "runtimes" : "health",
        completed,
        total,
      });
    }
    emitMockProgress({ scanId, phase: "complete", completed: total, total });
    return {
      ...mockScan,
      ...analyzeMockProjects(browserProjects),
      scanRoots: browserScanRoots,
      scanSettings: browserScanSettings,
      scannedAt: new Date().toISOString(),
    };
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
      browserProjects = [
        ...browserProjects,
        {
          name,
          path,
          ecosystems: [],
          lockFiles: [],
          runtimeRequirements: [],
          dependencies: [],
          warnings: ["浏览器预览模式未读取本机文件"],
        },
      ];
    }
    if (!browserScanRoots.includes(path)) browserScanRoots = [...browserScanRoots, path];
    return analyzeMockProjects(browserProjects);
  },
  async removeScanRoot(path) {
    browserProjects = browserProjects.filter(
      (project) => project.path !== path && !project.path.startsWith(`${path}/`),
    );
    browserScanRoots = browserScanRoots.filter((root) => root !== path);
    return analyzeMockProjects(browserProjects);
  },
  async getScanSettings() {
    return browserScanSettings;
  },
  async updateScanSettings(settings) {
    if (!Number.isInteger(settings.maxDepth) || settings.maxDepth < 1 || settings.maxDepth > 12)
      throw new Error("扫描范围设置无效：最大扫描深度需在 1 到 12 之间");
    browserScanSettings = {
      ...settings,
      ignoredPaths: [...new Set(settings.ignoredPaths)].sort(),
      defaultIgnoredDirectoryNames: [...mockScan.scanSettings.defaultIgnoredDirectoryNames],
    };
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
    return {
      ...mockSnapshotComparison,
      baseline: mockSnapshotSummaries.find((summary) => summary.id === baselineId) ?? mockSnapshotComparison.baseline,
      current: mockSnapshotSummaries.find((summary) => summary.id === currentId) ?? mockSnapshotComparison.current,
    };
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
  async searchPackageCatalog(searchId, managerId, query): Promise<CatalogSearchResponse> {
    if (browserScanSettings.networkPolicy !== "registry") {
      return {
        searchId,
        managerId,
        query,
        status: "offline",
        blockerCode: "NETWORK_POLICY_OFFLINE",
        message: "当前联网策略为离线。请在项目页将联网策略切换为“允许访问软件源”后再搜索。",
        results: [],
      };
    }
    await wait(120);
    if (cancelledMockCatalogSearches.delete(searchId)) {
      return {
        searchId,
        managerId,
        query,
        status: "cancelled",
        blockerCode: "CANCELLED",
        message: "目录搜索已取消，未生成安装计划。",
        results: [],
      };
    }
    const all =
      managerId === "homebrew"
        ? [
            { name: "ripgrep", description: "Search tool", version: "14.1.1" },
            { name: "jq", description: "Command-line JSON processor", version: "1.8.1" },
          ]
        : [
            { name: "eslint", description: "JavaScript linter", version: "9.30.0" },
            { name: "@typescript-eslint/parser", description: "TypeScript parser for ESLint", version: "8.35.0" },
          ];
    const results = all
      .filter((item) => item.name.toLowerCase().includes(query.toLowerCase()))
      .map((item) => {
        const installed = mockScan.packages.find((pkg) => pkg.managerId === managerId && pkg.name === item.name);
        return { managerId, ...item, installed: Boolean(installed), installedVersion: installed?.version };
      });
    return {
      searchId,
      managerId,
      query,
      status: "ready",
      message: "浏览器预览使用固定目录结果；选择结果只会填入安装目标。",
      results,
    };
  },
  async cancelPackageCatalogSearch(searchId) {
    cancelledMockCatalogSearches.add(searchId);
  },
  async planPackageAction(managerId, action, targets) {
    await wait(80);
    return createMockActionPlan(managerId, action, targets);
  },
  async getPackageActionCapabilities() {
    return (["homebrew", "npm", "pnpm"] as WritableManagerId[]).flatMap((managerId) =>
      (["install", "upgrade", "uninstall", "cleanup"] as PackageAction[]).map(
        (action): ActionCapability => ({
          managerId,
          action,
          ready: true,
          checks: [
            { code: "READY", status: "pass", title: "基础条件已满足", detail: "浏览器预览不会执行真实写操作。" },
            ...(managerId !== "homebrew" && action !== "cleanup"
              ? [
                  {
                    code: "SCRIPTS_DISABLED" as const,
                    status: "pass" as const,
                    title: "生命周期脚本已禁用",
                    detail: "固定使用 --ignore-scripts。",
                  },
                ]
              : []),
          ],
        }),
      ),
    );
  },
  async executePackageAction(planId) {
    const plan = mockActionPlans.get(planId);
    if (!plan) throw new Error("操作计划不存在、已执行或已过期");
    mockActionPlans.delete(planId);
    const actionId = crypto.randomUUID();
    const startedAt = new Date().toISOString();
    const managerName = plan.managerId === "homebrew" ? "Homebrew" : plan.managerId;
    for (const message of [
      `开始执行已确认的 ${managerName} 操作`,
      `${plan.managerId} ${actionLabel[plan.action]} 正在运行`,
      "正在重新扫描本机环境并计算变化",
    ]) {
      emitMockActionProgress({
        actionId,
        status: "running",
        message,
        cancellable: message !== "正在重新扫描本机环境并计算变化",
        timestamp: new Date().toISOString(),
      });
      await wait(100);
      if (cancelledMockActions.delete(actionId)) {
        const result: PackageActionResult = {
          actionId,
          planId,
          managerId: plan.managerId,
          action: plan.action,
          targets: plan.targets,
          status: "unknown",
          commandPreview: plan.commandPreview,
          logs: [message],
          error: "操作已终止；包管理器状态未知，已强制重新扫描。",
          comparison: mockActionComparison(plan),
          environment: { ...mockScan, scannedAt: new Date().toISOString() },
          startedAt,
          finishedAt: new Date().toISOString(),
        };
        mockActionAudit.unshift({
          actionId,
          planId,
          managerId: plan.managerId,
          action: plan.action,
          targets: plan.targets,
          status: result.status,
          commandPreview: result.commandPreview,
          logs: result.logs,
          error: result.error,
          startedAt,
          finishedAt: result.finishedAt,
          baselineSnapshotId: 2,
          resultSnapshotId: 3,
          observedOutcome: "ambiguous",
          evidence: ["浏览器预览：取消后结果需要重新核对"],
          reconciledAt: result.finishedAt,
          rescanRequired: false,
        });
        emitMockActionProgress({
          actionId,
          status: "unknown",
          message: "操作状态未知，环境已重新扫描",
          cancellable: false,
          timestamp: result.finishedAt,
        });
        return result;
      }
    }
    const result: PackageActionResult = {
      actionId,
      planId,
      managerId: plan.managerId,
      action: plan.action,
      targets: plan.targets,
      status: "succeeded",
      commandPreview: plan.commandPreview,
      logs: [`${plan.managerId} ${actionLabel[plan.action]} 完成`],
      comparison: mockActionComparison(plan),
      environment: { ...mockScan, scannedAt: new Date().toISOString() },
      startedAt,
      finishedAt: new Date().toISOString(),
    };
    mockActionAudit.unshift({
      actionId,
      planId,
      managerId: plan.managerId,
      action: plan.action,
      targets: plan.targets,
      status: result.status,
      commandPreview: result.commandPreview,
      logs: result.logs,
      startedAt,
      finishedAt: result.finishedAt,
      baselineSnapshotId: 2,
      resultSnapshotId: 3,
      observedOutcome: "applied",
      evidence: ["浏览器预览：已观察到固定变化"],
      reconciledAt: result.finishedAt,
      rescanRequired: false,
    });
    emitMockActionProgress({
      actionId,
      status: "succeeded",
      message: "操作完成，环境已重新扫描",
      cancellable: false,
      timestamp: result.finishedAt,
    });
    return result;
  },
  async cancelPackageAction(actionId) {
    cancelledMockActions.add(actionId);
  },
  async listenToPackageActionProgress(listener) {
    mockActionProgressListeners.add(listener);
    return () => mockActionProgressListeners.delete(listener);
  },
  async listPackageActionAudit() {
    return [...mockActionAudit];
  },
  async reconcilePackageAction(actionId) {
    const record = mockActionAudit.find((item) => item.actionId === actionId);
    if (!record) throw new Error("操作审计记录不存在");
    const audit = {
      ...record,
      status: record.status === "running" ? ("unknown" as const) : record.status,
      observedOutcome: "applied" as const,
      evidence: ["浏览器预览：重新扫描后目标状态符合预期"],
      reconciledAt: new Date().toISOString(),
      rescanRequired: false,
    };
    mockActionAudit.splice(mockActionAudit.indexOf(record), 1, audit);
    return { audit, environment: { ...mockScan, scannedAt: new Date().toISOString() } };
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
  compareSnapshots: (baselineId, currentId) =>
    invoke<SnapshotComparison>("compare_snapshots", { baselineId, currentId }),
  exportSnapshotComparisonReport: (format, baselineId, currentId) =>
    invoke("export_snapshot_comparison_report", { format, baselineId, currentId }),
  getHealthReport: () => invoke<HealthIssue[]>("get_health_report"),
  getScanLogs: () => invoke<TaskLog[]>("get_scan_logs"),
  getProjectDependencyGraph: (projectPath) =>
    invoke<ProjectDependencyGraph>("get_project_dependency_graph", { projectPath }),
  getProjectSupplyChainReport: (projectPath) =>
    invoke<ProjectSupplyChainReport>("get_project_supply_chain_report", { projectPath }),
  exportProjectSbom: (projectPath) => invoke("export_project_sbom", { projectPath }),
  searchPackageCatalog: (searchId, managerId, query) =>
    invoke<CatalogSearchResponse>("search_package_catalog", { searchId, managerId, query }),
  cancelPackageCatalogSearch: (searchId) => invoke<void>("cancel_package_catalog_search", { searchId }),
  planPackageAction: (managerId, action, targets) =>
    invoke<PackageActionPlan>("plan_package_action", { managerId, action, targets }),
  getPackageActionCapabilities: () => invoke<ActionCapability[]>("get_package_action_capabilities"),
  executePackageAction: (planId) => invoke<PackageActionResult>("execute_package_action", { planId }),
  cancelPackageAction: (actionId) => invoke<void>("cancel_package_action", { actionId }),
  async listenToPackageActionProgress(listener) {
    return listen<PackageActionProgress>("package-action-progress", (event) => listener(event.payload));
  },
  listPackageActionAudit: () => invoke<PackageActionAuditRecord[]>("list_package_action_audit"),
  reconcilePackageAction: (actionId) => invoke("reconcile_package_action", { actionId }),
};

export const api: DevPkgApi = new Proxy(tauriApi, {
  get(target, property: keyof DevPkgApi) {
    return isTauriRuntime() ? target[property] : mockApi[property];
  },
});
