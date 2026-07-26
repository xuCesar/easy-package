import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mockScan } from "../mock-data";
import { LogsPage } from "./LogsPage";

afterEach(cleanup);

describe("LogsPage", () => {
  it("按类别和状态筛选扫描日志", () => {
    const logs = [
      ...mockScan.logs,
      {
        id: "manager-success",
        category: "manager" as const,
        status: "success" as const,
        message: "Homebrew 检测完成",
        timestamp: "2026-07-12T00:00:00.000Z",
      },
      {
        id: "manager-warning",
        category: "manager" as const,
        status: "warning" as const,
        message: "npm 缓存目录部分不可访问",
        timestamp: "2026-07-12T00:01:00.000Z",
      },
    ];
    render(<LogsPage logs={logs} />);

    expect(screen.getByText("环境扫描完成")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("日志类别"), { target: { value: "manager" } });
    expect(screen.getByText("Homebrew 检测完成")).toBeInTheDocument();
    expect(screen.queryByText("环境扫描完成")).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("日志状态"), { target: { value: "warning" } });
    expect(screen.getByText("npm 缓存目录部分不可访问")).toBeInTheDocument();
    expect(screen.queryByText("Homebrew 检测完成")).not.toBeInTheDocument();
  });

  it("展示无匹配日志空态", () => {
    render(<LogsPage logs={mockScan.logs} />);

    fireEvent.change(screen.getByLabelText("日志类别"), { target: { value: "storage" } });
    fireEvent.change(screen.getByLabelText("日志状态"), { target: { value: "error" } });
    expect(screen.getByText("没有匹配的日志")).toBeInTheDocument();
  });

  it("复制诊断输出", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    const logs = [{ ...mockScan.logs[0], output: "diagnostic output" }];
    render(<LogsPage logs={logs} />);

    fireEvent.click(screen.getByRole("button", { name: "复制输出" }));
    expect(writeText).toHaveBeenCalledWith("diagnostic output");
    expect(await screen.findByRole("button", { name: "已复制" })).toBeInTheDocument();
  });

  it("复制失败时展示恢复提示", async () => {
    Object.assign(navigator, { clipboard: { writeText: vi.fn().mockRejectedValue(new Error("denied")) } });
    const logs = [{ ...mockScan.logs[0], output: "diagnostic output" }];
    render(<LogsPage logs={logs} />);

    fireEvent.click(screen.getByRole("button", { name: "复制输出" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("无法复制诊断输出");
  });
});
