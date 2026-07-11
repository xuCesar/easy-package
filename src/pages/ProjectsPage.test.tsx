import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ProjectsPage } from "./ProjectsPage";
import type { ProjectMetadata, ScanSettings } from "../types";

const project: ProjectMetadata = {
  name: "demo-app",
  path: "/tmp/demo-app",
  ecosystems: ["JavaScript"],
  lockFiles: ["pnpm-lock.yaml"],
  runtimeRequirements: [{ runtime: "Node.js", requirement: ">=22" }],
  packageManager: "pnpm@11",
  dependencies: [],
  warnings: [],
};

afterEach(cleanup);

const scanSettings: ScanSettings = {
  ignoredPaths: ["/tmp/archive"],
  maxDepth: 6,
  defaultIgnoredDirectoryNames: ["node_modules", ".git", "target", "dist", "build", ".venv", "vendor"],
};

const pageProps = {
  projects: [project],
  workspaces: [],
  scanRoots: [project.path],
  scanSettings,
  onAddRoot: vi.fn().mockResolvedValue(undefined),
  onRemoveRoot: vi.fn().mockResolvedValue(undefined),
  onUpdateSettings: vi.fn().mockResolvedValue(undefined),
  onExportReport: vi.fn().mockResolvedValue({ saved: true }),
  onRefresh: vi.fn(),
};

describe("ProjectsPage", () => {
  it("在浏览器预览中添加和移除扫描目录", async () => {
    const onAddRoot = vi.fn().mockResolvedValue(undefined);
    const onRemoveRoot = vi.fn().mockResolvedValue(undefined);

    render(<ProjectsPage {...pageProps} onAddRoot={onAddRoot} onRemoveRoot={onRemoveRoot} />);

    fireEvent.click(screen.getByRole("button", { name: "添加目录" }));
    await waitFor(() => expect(onAddRoot).toHaveBeenCalledWith("/Users/demo/Code/new-project"));

    fireEvent.click(screen.getByRole("button", { name: `移除扫描目录 ${project.path}` }));
    expect(onRemoveRoot).toHaveBeenCalledWith(project.path);
  });

  it("显示添加扫描目录失败原因", async () => {
    const onAddRoot = vi.fn().mockRejectedValue(new Error("目录不可读取"));

    render(<ProjectsPage {...pageProps} projects={[]} scanRoots={[]} onAddRoot={onAddRoot} />);

    fireEvent.click(screen.getByRole("button", { name: "添加目录" }));
    expect(await screen.findByText("目录不可读取")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "添加目录" })).toBeEnabled();
  });

  it("更新扫描深度并展示默认和用户忽略目录", async () => {
    const onUpdateSettings = vi.fn().mockResolvedValue(undefined);
    render(<ProjectsPage {...pageProps} onUpdateSettings={onUpdateSettings} />);

    expect(screen.getByText("node_modules")).toBeInTheDocument();
    expect(screen.getByText("/tmp/archive")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("最大扫描深度"), { target: { value: "8" } });
    fireEvent.click(screen.getByRole("button", { name: "应用" }));

    await waitFor(() => expect(onUpdateSettings).toHaveBeenCalledWith({ ...scanSettings, maxDepth: 8 }));
    expect(screen.getByText("扫描范围已更新，项目与依赖洞察已重新计算。")).toBeInTheDocument();
  });

  it("拒绝无效忽略路径并处理导出成功与失败", async () => {
    const onExportReport = vi.fn().mockResolvedValue({ saved: true });
    const view = render(<ProjectsPage {...pageProps} onExportReport={onExportReport} />);

    fireEvent.change(screen.getByLabelText("忽略目录"), { target: { value: "relative/path" } });
    fireEvent.click(screen.getByRole("button", { name: "添加忽略目录" }));
    expect(screen.getByText("请输入绝对目录路径。")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("报告格式"), { target: { value: "json" } });
    fireEvent.click(screen.getByRole("button", { name: "导出报告" }));
    await waitFor(() => expect(onExportReport).toHaveBeenCalledWith("json"));
    expect(screen.getByText("环境报告已导出。")).toBeInTheDocument();

    view.rerender(<ProjectsPage {...pageProps} onExportReport={vi.fn().mockRejectedValue(new Error("无法写入报告"))} />);
    fireEvent.click(screen.getByRole("button", { name: "导出报告" }));
    expect(await screen.findByText("无法写入报告")).toBeInTheDocument();
  });
});
