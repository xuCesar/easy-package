import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mockScan } from "../mock-data";
import { OverviewPage } from "./OverviewPage";

afterEach(cleanup);

describe("OverviewPage", () => {
  it("没有扫描目录时展示可选项目引导，不显示项目数字卡", () => {
    const onNavigate = vi.fn();
    render(
      <OverviewPage
        data={{ ...mockScan, projects: [], scanRoots: [] }}
        isLoading={false}
        onRefresh={vi.fn()}
        onCancel={vi.fn()}
        onNavigate={onNavigate}
        onStartUpgradePlan={vi.fn()}
      />,
    );

    const summary = screen.getByRole("region", { name: "本机环境摘要" });
    expect(within(summary).queryByText("已扫描项目")).not.toBeInTheDocument();
    expect(screen.getByText("添加代码目录后，可以检查项目与本机运行时是否匹配。")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /项目工作区（可选）/ }));

    expect(onNavigate).toHaveBeenCalledWith("projects");
  });

  it("已有扫描目录时隐藏项目引导，可写更新进入操作中心", () => {
    const onNavigate = vi.fn();
    const onStartUpgradePlan = vi.fn();
    render(
      <OverviewPage
        data={mockScan}
        isLoading={false}
        onRefresh={vi.fn()}
        onCancel={vi.fn()}
        onNavigate={onNavigate}
        onStartUpgradePlan={onStartUpgradePlan}
      />,
    );

    expect(screen.queryByText("添加代码目录后，可以检查项目与本机运行时是否匹配。")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /可更新 3 前往安全操作模式/ }));

    expect(onStartUpgradePlan).toHaveBeenCalledWith(
      expect.objectContaining({ managerId: "homebrew", targets: ["git"] }),
    );
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it("概览仅展示全局环境提醒，不展开项目问题", () => {
    const onNavigate = vi.fn();
    render(
      <OverviewPage
        data={mockScan}
        isLoading={false}
        onRefresh={vi.fn()}
        onCancel={vi.fn()}
        onNavigate={onNavigate}
        onStartUpgradePlan={vi.fn()}
      />,
    );

    const attention = screen.getByRole("region", { name: "待关注问题" });
    expect(within(attention).getByRole("heading", { name: "1 项需关注" })).toBeInTheDocument();

    fireEvent.click(within(attention).getByRole("button", { name: /3 个软件包可更新/ }));
    expect(onNavigate).toHaveBeenCalledWith("packages");
    expect(within(attention).queryByText(/api-lab 存在 1 项供应链风险/)).not.toBeInTheDocument();
  });

  it("离线且没有已知更新时展示未检查更新", () => {
    const data = {
      ...mockScan,
      packages: mockScan.packages.map((pkg) => ({ ...pkg, updateStatus: "unknown" as const })),
      healthIssues: [],
    };
    const onNavigate = vi.fn();
    render(
      <OverviewPage
        data={data}
        isLoading={false}
        onRefresh={vi.fn()}
        onCancel={vi.fn()}
        onNavigate={onNavigate}
        onStartUpgradePlan={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /可更新 — 未检查更新（当前离线）/ }));

    expect(onNavigate).toHaveBeenCalledWith("packages");
    expect(screen.queryByText("均为最新")).not.toBeInTheDocument();
  });

  it("联网检查后没有更新时展示均为最新", () => {
    const data = {
      ...mockScan,
      packages: mockScan.packages.map((pkg) => ({ ...pkg, updateStatus: "upToDate" as const })),
      scanSettings: { ...mockScan.scanSettings, networkPolicy: "registry" as const },
      healthIssues: [],
    };
    render(
      <OverviewPage
        data={data}
        isLoading={false}
        onRefresh={vi.fn()}
        onCancel={vi.fn()}
        onNavigate={vi.fn()}
        onStartUpgradePlan={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: /可更新 0 均为最新/ })).toBeInTheDocument();
  });

  it("没有待关注问题时展示一切正常", () => {
    const data = {
      ...mockScan,
      healthIssues: [],
      runtimeAssessments: [],
      projects: mockScan.projects.map((project) => ({ ...project, supplyChainRiskSummary: undefined })),
    };
    render(
      <OverviewPage
        data={data}
        isLoading={false}
        onRefresh={vi.fn()}
        onCancel={vi.fn()}
        onNavigate={vi.fn()}
        onStartUpgradePlan={vi.fn()}
      />,
    );

    const attention = screen.getByRole("region", { name: "待关注问题" });
    expect(within(attention).getByRole("heading", { name: "一切正常" })).toBeInTheDocument();
    expect(within(attention).getByText("未发现需要关注的问题，环境状态良好。")).toBeInTheDocument();
  });
});
