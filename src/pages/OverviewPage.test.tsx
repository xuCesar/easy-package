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
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "添加扫描目录" }));

    expect(onNavigate).toHaveBeenCalledWith("projects");
  });

  it("概览仅展示全局环境提醒，不展开项目问题", () => {
    const onNavigate = vi.fn();
    render(
      <OverviewPage data={mockScan} isLoading={false} onRefresh={vi.fn()} onCancel={vi.fn()} onNavigate={onNavigate} />,
    );

    const attention = screen.getByRole("region", { name: "待关注问题" });
    expect(within(attention).getByRole("heading", { name: "1 项需关注" })).toBeInTheDocument();

    fireEvent.click(within(attention).getByRole("button", { name: /3 个软件包可更新/ }));
    expect(onNavigate).toHaveBeenCalledWith("environment");
    expect(within(attention).queryByText(/api-lab 存在 1 项供应链风险/)).not.toBeInTheDocument();
  });

  it("没有待关注问题时展示一切正常", () => {
    const data = {
      ...mockScan,
      healthIssues: [],
      runtimeAssessments: [],
      projects: mockScan.projects.map((project) => ({ ...project, supplyChainRiskSummary: undefined })),
    };
    render(<OverviewPage data={data} isLoading={false} onRefresh={vi.fn()} onCancel={vi.fn()} onNavigate={vi.fn()} />);

    const attention = screen.getByRole("region", { name: "待关注问题" });
    expect(within(attention).getByRole("heading", { name: "一切正常" })).toBeInTheDocument();
    expect(within(attention).getByText("未发现需要关注的问题，环境状态良好。")).toBeInTheDocument();
  });
});
