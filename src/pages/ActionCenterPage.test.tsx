import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PackageActionPlan, PackageActionResult } from "../types";
import { ActionCenterPage } from "./ActionCenterPage";

const packages = [
  {
    id: "homebrew:git",
    managerId: "homebrew" as const,
    name: "git",
    version: "2.49.0",
    latestVersion: "2.50.1",
    scope: "system" as const,
    updateStatus: "available" as const,
  },
  {
    id: "homebrew:ripgrep",
    managerId: "homebrew" as const,
    name: "ripgrep",
    version: "14.1.1",
    latestVersion: "14.1.1",
    scope: "system" as const,
    updateStatus: "upToDate" as const,
  },
  {
    id: "npm:npm-check-updates",
    managerId: "npm" as const,
    name: "npm-check-updates",
    version: "18.0.0",
    latestVersion: "19.0.0",
    scope: "global" as const,
    updateStatus: "available" as const,
  },
  {
    id: "pnpm:typescript",
    managerId: "pnpm" as const,
    name: "typescript",
    version: "5.8.3",
    latestVersion: "5.9.3",
    scope: "global" as const,
    updateStatus: "available" as const,
  },
];

const plan: PackageActionPlan = {
  id: "plan-1",
  managerId: "homebrew",
  action: "install",
  targets: ["jq"],
  commandPreview: "/opt/homebrew/bin/brew install jq",
  warnings: ["该操作会修改本机 Homebrew 环境，无法保证自动回滚。"],
  previewLines: ["Formula 名称已通过严格语法校验：jq；存在性由 Homebrew 执行时验证。"],
  checks: [{ code: "READY" as const, status: "pass" as const, title: "基础条件已满足", detail: "仍需二次确认。" }],
  requiresNetwork: true,
  createdAt: "2026-07-11T00:00:00Z",
};

const baseProps = {
  packages,
  scanSettings: { ignoredPaths: [], maxDepth: 6, defaultIgnoredDirectoryNames: [], networkPolicy: "registry" as const },
  capabilities: (["homebrew", "npm", "pnpm"] as const).flatMap((managerId) =>
    (["install", "upgrade", "uninstall", "cleanup"] as const).map((action) => ({
      managerId,
      action,
      ready: true,
      checks: [{ code: "READY" as const, status: "pass" as const, title: "基础条件已满足", detail: "仍需二次确认。" }],
    })),
  ),
  audit: [],
  progress: [],
  isPlanning: false,
  isExecuting: false,
  isReconciling: false,
  isCatalogSearching: false,
  onSearchCatalog: vi.fn().mockResolvedValue(undefined),
  onCancelCatalogSearch: vi.fn().mockResolvedValue(undefined),
  onClearCatalogSearch: vi.fn(),
  onCreatePlan: vi.fn().mockResolvedValue(plan),
  onExecute: vi.fn().mockResolvedValue(undefined),
  onCancel: vi.fn().mockResolvedValue(undefined),
  onReconcile: vi.fn().mockResolvedValue(undefined),
  onClearPlan: vi.fn(),
  onUpdateSettings: vi.fn().mockResolvedValue(undefined),
  onRefresh: vi.fn(),
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("ActionCenterPage", () => {
  it("待处理区按管理器预填可更新包并保留现有计划流程", () => {
    render(
      <ActionCenterPage
        {...baseProps}
        upgradePrefill={{
          managerId: "homebrew",
          targets: ["git"],
          truncatedCount: 0,
          otherWritableCount: 0,
          unwritableCount: 0,
        }}
      />,
    );

    expect(screen.getByRole("region", { name: "待处理操作" })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("已从软件包页带入 1 个 Homebrew 升级目标");
    expect(screen.getByRole("button", { name: "处理 Homebrew 的 1 个可更新项" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "处理 npm 的 1 个可更新项" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "处理 pnpm 的 1 个可更新项" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "处理 npm 的 1 个可更新项" }));

    expect(screen.getByRole("tab", { name: "npm" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "升级" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("checkbox", { name: /npm-check-updates/ })).toBeChecked();
    expect(screen.queryByText(/已从软件包页带入/)).not.toBeInTheDocument();
    expect(baseProps.onCreatePlan).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "生成操作计划" }));
    expect(baseProps.onCreatePlan).toHaveBeenCalledWith("npm", "upgrade", ["npm-check-updates"]);
  });

  it("离线时只说明未检查更新，不展示伪造的待更新任务", () => {
    render(<ActionCenterPage {...baseProps} scanSettings={{ ...baseProps.scanSettings, networkPolicy: "offline" }} />);

    expect(screen.getByRole("region", { name: "待处理操作" })).toHaveTextContent("未检查更新");
    expect(screen.queryByRole("button", { name: /处理 .* 个可更新项/ })).not.toBeInTheDocument();
  });

  it("生成安装预检并要求二次确认后才能执行", () => {
    const { rerender } = render(<ActionCenterPage {...baseProps} />);
    fireEvent.change(screen.getByLabelText("待安装 Formula"), { target: { value: "jq" } });
    fireEvent.click(screen.getByRole("button", { name: "生成操作计划" }));
    expect(baseProps.onCreatePlan).toHaveBeenCalledWith("homebrew", "install", ["jq"]);

    rerender(<ActionCenterPage {...baseProps} plan={plan} />);
    expect(screen.getByText("/opt/homebrew/bin/brew install jq")).toBeInTheDocument();
    const execute = screen.getByRole("button", { name: "确认并执行" });
    expect(execute).toBeDisabled();
    fireEvent.click(screen.getByLabelText(/我已核对管理器/));
    expect(execute).toBeEnabled();
    fireEvent.click(execute);
    expect(baseProps.onExecute).toHaveBeenCalledOnce();
  });

  it("仅从扫描结果选择批量升级和卸载目标", () => {
    render(<ActionCenterPage {...baseProps} />);
    fireEvent.click(screen.getByRole("tab", { name: "升级" }));
    fireEvent.click(screen.getByLabelText(/git/));
    fireEvent.click(screen.getByRole("button", { name: "生成操作计划" }));
    expect(baseProps.onCreatePlan).toHaveBeenLastCalledWith("homebrew", "upgrade", ["git"]);

    fireEvent.click(screen.getByRole("tab", { name: "卸载" }));
    fireEvent.change(screen.getByLabelText("待卸载 Formula"), { target: { value: "ripgrep" } });
    fireEvent.click(screen.getByRole("button", { name: "生成操作计划" }));
    expect(baseProps.onCreatePlan).toHaveBeenLastCalledWith("homebrew", "uninstall", ["ripgrep"]);
  });

  it("为 pnpm 生成固定禁用脚本的全局包计划", () => {
    render(<ActionCenterPage {...baseProps} />);
    fireEvent.click(screen.getByRole("tab", { name: "pnpm" }));
    fireEvent.change(screen.getByLabelText("待安装 pnpm 全局包"), { target: { value: "eslint" } });
    fireEvent.click(screen.getByRole("button", { name: "生成操作计划" }));
    expect(baseProps.onCreatePlan).toHaveBeenLastCalledWith("pnpm", "install", ["eslint"]);

    fireEvent.click(screen.getByRole("tab", { name: "升级" }));
    fireEvent.click(screen.getByLabelText(/typescript/));
    fireEvent.click(screen.getByRole("button", { name: "生成操作计划" }));
    expect(baseProps.onCreatePlan).toHaveBeenLastCalledWith("pnpm", "upgrade", ["typescript"]);
  });

  it("为 npm 生成受控全局包计划并区分缓存校验语义", () => {
    render(<ActionCenterPage {...baseProps} />);
    fireEvent.click(screen.getByRole("tab", { name: "npm" }));
    fireEvent.change(screen.getByLabelText("待安装 npm 全局包"), { target: { value: "eslint" } });
    fireEvent.click(screen.getByRole("button", { name: "生成操作计划" }));
    expect(baseProps.onCreatePlan).toHaveBeenLastCalledWith("npm", "install", ["eslint"]);

    fireEvent.click(screen.getByRole("tab", { name: "缓存校验与回收" }));
    expect(screen.getByText(/不会执行 npm cache clean --force/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "生成操作计划" }));
    expect(baseProps.onCreatePlan).toHaveBeenLastCalledWith("npm", "cleanup", []);
  });

  it("目录搜索结果仅填充安装目标，仍需生成计划", () => {
    const catalogResponse = {
      searchId: "search-1",
      managerId: "homebrew" as const,
      query: "jq",
      status: "ready" as const,
      message: "目录搜索仅用于选择安装目标；不会生成计划或执行安装。",
      results: [
        {
          managerId: "homebrew" as const,
          name: "jq",
          description: "JSON processor",
          version: "1.8.1",
          installed: false,
        },
      ],
    };
    render(<ActionCenterPage {...baseProps} catalogResponse={catalogResponse} />);
    fireEvent.change(screen.getByLabelText("Homebrew 目录搜索词"), { target: { value: "jq" } });
    fireEvent.click(screen.getByRole("button", { name: "搜索目录" }));
    expect(baseProps.onSearchCatalog).toHaveBeenCalledWith("homebrew", "jq");
    fireEvent.click(screen.getByRole("button", { name: "用作安装目标" }));
    expect(screen.getByLabelText("待安装 Formula")).toHaveValue("jq");
    expect(baseProps.onCreatePlan).not.toHaveBeenCalled();
  });

  it("在离线策略下解释并禁用目录搜索，可显式允许检查更新", async () => {
    const catalogResponse = {
      searchId: "search-1",
      managerId: "npm" as const,
      query: "eslint",
      status: "offline" as const,
      blockerCode: "NETWORK_POLICY_OFFLINE" as const,
      message: "当前联网策略为离线。",
      results: [],
    };
    render(
      <ActionCenterPage
        {...baseProps}
        scanSettings={{ ...baseProps.scanSettings, networkPolicy: "offline" }}
        catalogResponse={catalogResponse}
      />,
    );
    expect(screen.getByText("NETWORK_POLICY_OFFLINE")).toBeInTheDocument();
    expect(screen.getByText(/当前为离线模式；允许检查更新并重新扫描后才能搜索目录/)).toBeInTheDocument();
    expect(screen.getByLabelText("Homebrew 目录搜索词")).toBeDisabled();
    expect(screen.getByRole("button", { name: "搜索目录" })).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "允许检查更新" }));
    await waitFor(() =>
      expect(baseProps.onUpdateSettings).toHaveBeenCalledWith({
        ...baseProps.scanSettings,
        networkPolicy: "registry",
      }),
    );
    expect(baseProps.onRefresh).not.toHaveBeenCalled();
    expect(screen.getByText(/需要重新扫描后才会显示可更新状态/)).toBeInTheDocument();
  });

  it("阻止中断操作恢复前生成新计划并支持审计筛选", () => {
    const interrupted = {
      ...plan,
      actionId: "action-running",
      planId: plan.id,
      status: "running" as const,
      logs: [],
      startedAt: "2026-07-11T00:00:00Z",
      finishedAt: "2026-07-11T00:00:00Z",
      evidence: [],
      rescanRequired: true,
    };
    render(<ActionCenterPage {...baseProps} audit={[interrupted]} />);
    const pending = screen.getByRole("region", { name: "待处理操作" });
    const builder = screen.getByRole("region", { name: "操作预检设置" });
    expect(pending).toHaveTextContent("上次操作可能没做完");
    expect(pending.compareDocumentPosition(builder) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByRole("button", { name: "生成操作计划" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "重新扫描并核对" }));
    expect(baseProps.onReconcile).toHaveBeenCalledWith("action-running");
    fireEvent.change(screen.getByLabelText("审计管理器筛选"), { target: { value: "pnpm" } });
    expect(screen.getByText("没有匹配的写操作记录")).toBeInTheDocument();
  });

  it("展示稳定阻塞码并在计划前禁用不可用动作", () => {
    const capabilities = baseProps.capabilities.map((capability) =>
      capability.managerId === "homebrew" && capability.action === "install"
        ? {
            ...capability,
            ready: false,
            checks: [
              {
                code: "UNTRUSTED_EXECUTABLE" as const,
                status: "blocked" as const,
                title: "可执行文件不受信任",
                detail: "路径不在允许目录。",
              },
            ],
          }
        : capability,
    );
    render(<ActionCenterPage {...baseProps} capabilities={capabilities} />);
    expect(screen.getByText("当前操作被阻止")).toBeInTheDocument();
    expect(screen.getByText("UNTRUSTED_EXECUTABLE")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "生成操作计划" })).toBeDisabled();
  });

  it("展示清理预览、执行进度、状态未知结果和审计记录", () => {
    const cleanupPlan = {
      ...plan,
      action: "cleanup" as const,
      targets: [],
      commandPreview: "/opt/homebrew/bin/brew cleanup",
      previewLines: ["Would remove: cache.tar.gz"],
      requiresNetwork: false,
    };
    const result: PackageActionResult = {
      actionId: "action-1",
      planId: cleanupPlan.id,
      managerId: "homebrew",
      action: "cleanup",
      targets: [],
      status: "unknown",
      commandPreview: cleanupPlan.commandPreview,
      logs: ["cleanup running"],
      error: "操作已终止；包管理器状态未知，已强制重新扫描。",
      startedAt: "2026-07-11T00:00:00Z",
      finishedAt: "2026-07-11T00:00:02Z",
      comparison: {
        baseline: { id: 1, scannedAt: "old", managerCount: 1, packageCount: 2, projectCount: 0, healthIssueCount: 0 },
        current: { id: 2, scannedAt: "new", managerCount: 1, packageCount: 2, projectCount: 0, healthIssueCount: 0 },
        addedCount: 0,
        removedCount: 0,
        changedCount: 1,
        changes: [
          {
            kind: "changed",
            entity: "manager",
            key: "homebrew",
            title: "Homebrew 缓存信息已变化",
            description: "缓存占用已更新。",
          },
        ],
      },
    };
    const audit = [
      {
        actionId: result.actionId,
        planId: result.planId,
        managerId: result.managerId,
        action: result.action,
        targets: result.targets,
        status: result.status,
        commandPreview: result.commandPreview,
        logs: result.logs,
        error: result.error,
        startedAt: result.startedAt,
        finishedAt: result.finishedAt,
        observedOutcome: "ambiguous" as const,
        evidence: ["缓存大小未知"],
        rescanRequired: false,
      },
    ];
    const { rerender } = render(
      <ActionCenterPage
        {...baseProps}
        plan={cleanupPlan}
        result={result}
        audit={audit}
        progress={[
          {
            actionId: "action-1",
            status: "running",
            message: "cleanup running",
            cancellable: true,
            timestamp: "2026-07-11T00:00:01Z",
          },
        ]}
        isExecuting
      />,
    );
    expect(screen.getByText("Would remove: cache.tar.gz")).toBeInTheDocument();
    expect(screen.getByText("cleanup running")).toBeInTheDocument();
    expect(screen.getAllByText("状态未知").length).toBeGreaterThan(0);
    expect(screen.getByText("Homebrew 缓存信息已变化")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "查看详情" }));
    expect(screen.getByText("缓存大小未知")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "终止操作" }));
    expect(baseProps.onCancel).toHaveBeenCalledOnce();
    rerender(
      <ActionCenterPage
        {...baseProps}
        plan={cleanupPlan}
        result={result}
        audit={audit}
        progress={[
          {
            actionId: "action-1",
            status: "running",
            message: "正在重新扫描本机环境并计算变化",
            cancellable: false,
            timestamp: "2026-07-11T00:00:02Z",
          },
        ]}
        isExecuting
      />,
    );
    expect(screen.getByRole("button", { name: "终止操作" })).toBeDisabled();
    expect(screen.getByText("正在完成强制复扫，此阶段不能取消。")).toBeInTheDocument();
  });
});
