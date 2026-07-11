import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ScanSettings } from "../types";
import { SettingsPage } from "./SettingsPage";

const scanSettings: ScanSettings = {
  ignoredPaths: ["/tmp/archive"],
  maxDepth: 6,
  defaultIgnoredDirectoryNames: ["node_modules", ".git", "target", "dist", "build", ".venv", "vendor"],
  networkPolicy: "offline",
};

afterEach(cleanup);

describe("SettingsPage", () => {
  it("更新全局扫描设置", async () => {
    const onUpdateSettings = vi.fn().mockResolvedValue(undefined);
    render(<SettingsPage scanSettings={scanSettings} onUpdateSettings={onUpdateSettings} onExportReport={vi.fn()} />);

    expect(screen.getByText("node_modules")).toBeInTheDocument();
    expect(screen.getByText("/tmp/archive")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("最大扫描深度"), { target: { value: "8" } });
    fireEvent.click(screen.getByRole("button", { name: "应用" }));

    await waitFor(() => expect(onUpdateSettings).toHaveBeenCalledWith({ ...scanSettings, maxDepth: 8 }));
    expect(screen.getByText("系统扫描设置已更新。")).toBeInTheDocument();
  });

  it("校验忽略路径并导出环境报告", async () => {
    const onExportReport = vi.fn().mockResolvedValue({ saved: true });
    render(<SettingsPage scanSettings={scanSettings} onUpdateSettings={vi.fn()} onExportReport={onExportReport} />);

    fireEvent.change(screen.getByLabelText("忽略目录"), { target: { value: "relative/path" } });
    fireEvent.click(screen.getByRole("button", { name: "添加忽略目录" }));
    expect(screen.getByText("请输入绝对目录路径。")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("报告格式"), { target: { value: "json" } });
    fireEvent.click(screen.getByRole("button", { name: "导出报告" }));
    await waitFor(() => expect(onExportReport).toHaveBeenCalledWith("json"));
    expect(screen.getByText("环境报告已导出。")).toBeInTheDocument();
  });
});
