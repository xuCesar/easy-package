import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mockScan } from "../mock-data";
import { OverviewPage } from "./OverviewPage";

afterEach(cleanup);

describe("OverviewPage", () => {
  it("没有扫描项目时展示添加入口", () => {
    const onNavigate = vi.fn();
    render(
      <OverviewPage
        data={{ ...mockScan, projects: [] }}
        isLoading={false}
        onRefresh={vi.fn()}
        onCancel={vi.fn()}
        onNavigate={onNavigate}
        onOpenAnalysis={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "添加扫描目录" }));

    expect(onNavigate).toHaveBeenCalledWith("projects");
  });

  it("展示待关注问题聚合列表并可跳转对应页面", () => {
    const onNavigate = vi.fn();
    const onOpenAnalysis = vi.fn();
    render(
      <OverviewPage
        data={mockScan}
        isLoading={false}
        onRefresh={vi.fn()}
        onCancel={vi.fn()}
        onNavigate={onNavigate}
        onOpenAnalysis={onOpenAnalysis}
      />,
    );

    const attention = screen.getByRole("region", { name: "待关注问题" });
    expect(within(attention).getByRole("heading", { name: "2 项需关注" })).toBeInTheDocument();

    fireEvent.click(within(attention).getByRole("button", { name: /3 个软件包可更新/ }));
    expect(onNavigate).toHaveBeenCalledWith("environment");

    fireEvent.click(within(attention).getByRole("button", { name: /api-lab 存在 1 项供应链风险/ }));
    expect(onOpenAnalysis).toHaveBeenCalledWith("supplyChain");
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
        onOpenAnalysis={vi.fn()}
      />,
    );

    const attention = screen.getByRole("region", { name: "待关注问题" });
    expect(within(attention).getByRole("heading", { name: "一切正常" })).toBeInTheDocument();
    expect(within(attention).getByText("未发现需要关注的问题，环境状态良好。")).toBeInTheDocument();
  });
});
