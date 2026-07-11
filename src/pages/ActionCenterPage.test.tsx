import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ActionCenterPage } from "./ActionCenterPage";
import type { PackageActionPlan, PackageActionResult } from "../types";

const packages = [
  { id: "homebrew:git", managerId: "homebrew" as const, name: "git", version: "2.49.0", latestVersion: "2.50.1", scope: "system" as const, updateStatus: "available" as const },
  { id: "homebrew:ripgrep", managerId: "homebrew" as const, name: "ripgrep", version: "14.1.1", latestVersion: "14.1.1", scope: "system" as const, updateStatus: "upToDate" as const },
  { id: "pnpm:typescript", managerId: "pnpm" as const, name: "typescript", version: "5.8.3", latestVersion: "5.9.3", scope: "global" as const, updateStatus: "available" as const },
];

const plan: PackageActionPlan = {
  id: "plan-1", managerId: "homebrew", action: "install", targets: ["jq"], commandPreview: "/opt/homebrew/bin/brew install jq",
  warnings: ["该操作会修改本机 Homebrew 环境，无法保证自动回滚。"], previewLines: ["Formula 名称已通过严格语法校验：jq；存在性由 Homebrew 执行时验证。"], requiresNetwork: true, createdAt: "2026-07-11T00:00:00Z",
};

const baseProps = {
  packages,
  audit: [],
  progress: [],
  isPlanning: false,
  isExecuting: false,
  onCreatePlan: vi.fn().mockResolvedValue(plan),
  onExecute: vi.fn().mockResolvedValue(undefined),
  onCancel: vi.fn().mockResolvedValue(undefined),
  onClearPlan: vi.fn(),
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("ActionCenterPage", () => {
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

  it("阻止中断操作恢复前生成新计划并支持审计筛选", () => {
    const interrupted = { ...plan, actionId: "action-running", planId: plan.id, status: "running" as const, logs: [], startedAt: "2026-07-11T00:00:00Z", finishedAt: "2026-07-11T00:00:00Z" };
    render(<ActionCenterPage {...baseProps} audit={[interrupted]} />);
    expect(screen.getByText(/上次操作可能中断/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "生成操作计划" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("审计管理器筛选"), { target: { value: "pnpm" } });
    expect(screen.getByText("没有匹配的写操作记录")).toBeInTheDocument();
  });

  it("展示清理预览、执行进度、状态未知结果和审计记录", () => {
    const cleanupPlan = { ...plan, action: "cleanup" as const, targets: [], commandPreview: "/opt/homebrew/bin/brew cleanup", previewLines: ["Would remove: cache.tar.gz"], requiresNetwork: false };
    const result: PackageActionResult = {
      actionId: "action-1", planId: cleanupPlan.id, managerId: "homebrew", action: "cleanup", targets: [], status: "unknown", commandPreview: cleanupPlan.commandPreview,
      logs: ["cleanup running"], error: "操作已终止；包管理器状态未知，已强制重新扫描。", startedAt: "2026-07-11T00:00:00Z", finishedAt: "2026-07-11T00:00:02Z",
      comparison: { baseline: { id: 1, scannedAt: "old", managerCount: 1, packageCount: 2, projectCount: 0, healthIssueCount: 0 }, current: { id: 2, scannedAt: "new", managerCount: 1, packageCount: 2, projectCount: 0, healthIssueCount: 0 }, addedCount: 0, removedCount: 0, changedCount: 1, changes: [{ kind: "changed", entity: "manager", key: "homebrew", title: "Homebrew 缓存信息已变化", description: "缓存占用已更新。" }] },
    };
    const audit = [{ actionId: result.actionId, planId: result.planId, managerId: result.managerId, action: result.action, targets: result.targets, status: result.status, commandPreview: result.commandPreview, logs: result.logs, error: result.error, startedAt: result.startedAt, finishedAt: result.finishedAt }];
    const { rerender } = render(<ActionCenterPage {...baseProps} plan={cleanupPlan} result={result} audit={audit} progress={[{ actionId: "action-1", status: "running", message: "cleanup running", cancellable: true, timestamp: "2026-07-11T00:00:01Z" }]} isExecuting />);
    expect(screen.getByText("Would remove: cache.tar.gz")).toBeInTheDocument();
    expect(screen.getByText("cleanup running")).toBeInTheDocument();
    expect(screen.getAllByText("状态未知").length).toBeGreaterThan(0);
    expect(screen.getByText("Homebrew 缓存信息已变化")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "终止操作" }));
    expect(baseProps.onCancel).toHaveBeenCalledOnce();
    rerender(<ActionCenterPage {...baseProps} plan={cleanupPlan} result={result} audit={audit} progress={[{ actionId: "action-1", status: "running", message: "正在重新扫描本机环境并计算变化", cancellable: false, timestamp: "2026-07-11T00:00:02Z" }]} isExecuting />);
    expect(screen.getByRole("button", { name: "终止操作" })).toBeDisabled();
    expect(screen.getByText("正在完成强制复扫，此阶段不能取消。")).toBeInTheDocument();
  });
});
