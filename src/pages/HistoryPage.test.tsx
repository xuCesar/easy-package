import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SnapshotComparison, SnapshotSummary } from "../types";
import { HistoryPage } from "./HistoryPage";

const summaries: SnapshotSummary[] = [
  { id: 2, scannedAt: "2026-07-11T09:00:00Z", managerCount: 3, packageCount: 8, projectCount: 2, healthIssueCount: 1 },
  { id: 1, scannedAt: "2026-07-11T08:00:00Z", managerCount: 2, packageCount: 7, projectCount: 1, healthIssueCount: 2 },
];

const comparison: SnapshotComparison = {
  baseline: summaries[1],
  current: summaries[0],
  addedCount: 1,
  removedCount: 1,
  changedCount: 1,
  changes: [
    { kind: "added", entity: "package", key: "npm:test", title: "新增软件包：test", description: "npm · 1.0" },
    {
      kind: "changed",
      entity: "project",
      key: "/tmp/app",
      title: "项目元数据已变化：app",
      description: "锁文件已变化。",
    },
    { kind: "removed", entity: "health", key: "legacy", title: "健康提示已消失：旧版", description: "已消失。" },
  ],
};

afterEach(cleanup);

describe("HistoryPage", () => {
  it("按类型筛选变化，并比较指定快照", async () => {
    const onCompare = vi.fn().mockResolvedValue(comparison);
    render(
      <HistoryPage
        summaries={summaries}
        comparison={comparison}
        isLoading={false}
        onCompare={onCompare}
        onExport={vi.fn()}
      />,
    );

    fireEvent.change(screen.getByLabelText("变化类型"), { target: { value: "package" } });
    expect(screen.getByText("新增软件包：test")).toBeInTheDocument();
    expect(screen.queryByText("项目元数据已变化：app")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "比较快照" }));
    await waitFor(() => expect(onCompare).toHaveBeenCalledWith(1, 2));
  });

  it("拒绝倒序快照并处理变化报告导出", async () => {
    const onExport = vi.fn().mockResolvedValue({ saved: true });
    render(
      <HistoryPage
        summaries={summaries}
        comparison={comparison}
        isLoading={false}
        onCompare={vi.fn()}
        onExport={onExport}
      />,
    );

    fireEvent.change(screen.getByLabelText("当前快照"), { target: { value: "1" } });
    fireEvent.click(screen.getByRole("button", { name: "比较快照" }));
    expect(screen.getByText("基线快照必须早于当前快照。")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("变化报告格式"), { target: { value: "json" } });
    fireEvent.click(screen.getByRole("button", { name: "导出变化报告" }));
    await waitFor(() => expect(onExport).toHaveBeenCalledWith("json", 1, 2));
    expect(screen.getByText("变化报告已导出。")).toBeInTheDocument();
  });
});
