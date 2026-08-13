import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App";

afterEach(cleanup);

async function scanAndWaitForOverview() {
  const scanButton = await screen.findByRole("button", { name: "扫描" });
  await waitFor(() => expect(scanButton).toBeEnabled());
  fireEvent.click(scanButton);
  expect(screen.getByRole("button", { name: "取消扫描" })).toBeInTheDocument();
  expect(await screen.findByRole("status")).toHaveTextContent("正在读取包管理器 0/13");
  await waitFor(() => expect(screen.getByRole("heading", { name: "本机开发环境" })).toBeInTheDocument());
}

describe("App", () => {
  it("加载后展示概览并支持页面导航", async () => {
    const { container } = render(<App />);
    expect(container.querySelector(".window-controls")).not.toBeInTheDocument();
    expect(container.querySelector(".app-topbar")).toHaveAttribute("data-tauri-drag-region");
    expect(screen.getByRole("button", { name: "返回概览" })).toBeInTheDocument();
    expect(screen.queryByText("Easy Package")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "扫描" })).toBeDisabled();
    expect(screen.getByText(/读取本机包管理器、全局软件包、命令来源和运行时安装/)).toBeInTheDocument();
    expect(screen.getByText(/不会修改任何文件/)).toBeInTheDocument();
    expect(screen.queryByText("正在扫描本机环境")).not.toBeInTheDocument();
    await scanAndWaitForOverview();
    expect(screen.getByText(/浏览器预览：当前展示模拟数据/)).toBeInTheDocument();
    const mainNavigation = within(screen.getByRole("navigation", { name: "主要导航" }));
    const localWorkspace = within(screen.getByRole("group", { name: "本机工作区" }));
    const projectWorkspace = within(screen.getByRole("group", { name: "项目工作区" }));
    expect(mainNavigation.queryByRole("button", { name: "诊断" })).not.toBeInTheDocument();
    fireEvent.click(localWorkspace.getByRole("button", { name: "软件包" }));
    expect(screen.getByRole("heading", { name: "软件包" })).toBeInTheDocument();
    fireEvent.click(localWorkspace.getByRole("button", { name: "环境" }));
    const localWorkspaceNavigation = within(screen.getByRole("navigation", { name: "本机工作区导航" }));
    expect(localWorkspaceNavigation.queryByRole("button", { name: "项目分析" })).not.toBeInTheDocument();
    fireEvent.click(localWorkspaceNavigation.getByRole("button", { name: "运行时" }));
    expect(screen.getByRole("heading", { name: "运行时" })).toBeInTheDocument();
    fireEvent.click(localWorkspaceNavigation.getByRole("button", { name: "日志" }));
    expect(screen.getByRole("heading", { name: "日志" })).toBeInTheDocument();
    fireEvent.click(projectWorkspace.getByRole("button", { name: "项目" }));
    expect(screen.getByRole("heading", { name: "项目" })).toBeInTheDocument();
    const projectWorkspaceNavigation = within(screen.getByRole("navigation", { name: "项目工作区导航" }));
    expect(projectWorkspaceNavigation.getAllByRole("button")).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "查看项目 developer-tools" }));
    expect(screen.getByRole("heading", { name: "developer-tools" })).toBeInTheDocument();
    expect(projectWorkspace.getByRole("button", { name: "项目" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByText("运行时匹配")).toBeInTheDocument();
    expect(screen.getByText("锁文件问题")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "查看锁文件问题" }));
    fireEvent.click(screen.getByRole("button", { name: "检查锁文件问题" }));
    expect((await screen.findAllByText("依赖来源无法规范化")).length).toBeGreaterThan(0);
    fireEvent.click(localWorkspace.getByRole("button", { name: "软件包" }));
    fireEvent.click(screen.getByRole("button", { name: "管理操作" }));
    expect(screen.getByRole("heading", { name: "操作中心" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "缓存清理" }));
    const planButton = screen.getByRole("button", { name: "生成操作计划" });
    await waitFor(() => expect(planButton).toBeEnabled());
    fireEvent.click(planButton);
    expect(await screen.findByText("/opt/homebrew/bin/brew cleanup")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "系统设置" }));
    expect(screen.getByRole("heading", { name: "系统设置" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "系统扫描设置" })).toBeInTheDocument();
  });

  it("操作中心可从顶部导航与「安全操作模式」入口直达", async () => {
    render(<App />);
    await scanAndWaitForOverview();
    fireEvent.click(within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "操作" }));
    expect(screen.getByRole("heading", { name: "操作中心" })).toBeInTheDocument();
    expect(
      within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "操作" }),
    ).toHaveAttribute("aria-current", "page");
    fireEvent.click(within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "概览" }));
    expect(screen.getByRole("heading", { name: "本机开发环境" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "安全操作模式" }));
    expect(screen.getByRole("heading", { name: "操作中心" })).toBeInTheDocument();
  });

  it("概览可更新入口进入已预填的升级流程", async () => {
    render(<App />);
    await scanAndWaitForOverview();

    fireEvent.click(screen.getByRole("button", { name: /可更新 3 前往安全操作模式/ }));

    expect(screen.getByRole("heading", { name: "操作中心" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "升级" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("checkbox", { name: /git/ })).toBeChecked();
  });

  it("软件包页可更新筛选可一键预填批量升级计划", async () => {
    render(<App />);
    await scanAndWaitForOverview();
    fireEvent.click(
      within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "软件包" }),
    );
    fireEvent.change(screen.getByLabelText("状态"), { target: { value: "available" } });

    const bridgeButton = screen.getByRole("button", { name: "批量生成升级计划（1）" });
    expect(bridgeButton).toBeEnabled();
    fireEvent.click(bridgeButton);

    expect(screen.getByRole("heading", { name: "操作中心" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "升级" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Homebrew" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("checkbox", { name: /git/ })).toBeChecked();
    const notice = screen.getByRole("status");
    expect(notice).toHaveTextContent("已从软件包页带入 1 个 Homebrew 升级目标");
    expect(notice).toHaveTextContent("另有 1 个可更新包属于其他可写管理器");
    expect(notice).toHaveTextContent("1 个可更新包不属于受控可写管理器，已被过滤");
  });

  it("软件包页单行升级只预填当前软件包", async () => {
    render(<App />);
    await scanAndWaitForOverview();
    fireEvent.click(
      within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "软件包" }),
    );

    fireEvent.click(screen.getByRole("button", { name: "升级 git" }));

    expect(screen.getByRole("heading", { name: "操作中心" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "升级" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Homebrew" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("checkbox", { name: /git/ })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: /ripgrep/ })).not.toBeChecked();
    expect(screen.getByRole("status")).toHaveTextContent("已从软件包页带入 1 个 Homebrew 升级目标");
  });

  it("无可写管理器可更新包时批量升级入口禁用", async () => {
    render(<App />);
    await scanAndWaitForOverview();
    fireEvent.click(
      within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "软件包" }),
    );
    fireEvent.change(screen.getByLabelText("管理器"), { target: { value: "pip" } });

    expect(screen.getByRole("button", { name: "批量生成升级计划" })).toBeDisabled();
  });

  it("软件包筛选展示空态", async () => {
    render(<App />);
    await scanAndWaitForOverview();
    fireEvent.click(
      within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "软件包" }),
    );
    fireEvent.change(screen.getByPlaceholderText("搜索软件包"), { target: { value: "not-a-real-package" } });
    expect(await screen.findByText("没有匹配的软件包")).toBeInTheDocument();
  });

  it("按文本、管理器和更新状态组合筛选软件包", async () => {
    render(<App />);
    await scanAndWaitForOverview();
    fireEvent.click(
      within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "软件包" }),
    );
    fireEvent.change(screen.getByPlaceholderText("搜索软件包"), { target: { value: "type" } });
    fireEvent.change(screen.getByLabelText("管理器"), { target: { value: "npm" } });
    fireEvent.change(screen.getByLabelText("状态"), { target: { value: "available" } });

    expect(screen.getByText("1 个结果")).toBeInTheDocument();
    expect(screen.getByText("typescript")).toBeInTheDocument();
    expect(screen.queryByText("git")).not.toBeInTheDocument();
  });

  it("可按 RubyGems 和 Composer 筛选只读全局包", async () => {
    render(<App />);
    await scanAndWaitForOverview();
    fireEvent.click(
      within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "软件包" }),
    );

    fireEvent.change(screen.getByLabelText("管理器"), { target: { value: "rubygems" } });
    expect(screen.getByText("rake")).toBeInTheDocument();
    expect(screen.getByText("RubyGems", { selector: ".manager-chip" })).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("管理器"), { target: { value: "composer" } });
    expect(screen.getByText("psr/log")).toBeInTheDocument();
    expect(screen.getByText("Composer", { selector: ".manager-chip" })).toBeInTheDocument();
  });

  it("取消刷新后保留已有扫描结果", async () => {
    render(<App />);
    await scanAndWaitForOverview();

    fireEvent.click(screen.getByRole("button", { name: "刷新" }));
    fireEvent.click(screen.getByRole("button", { name: "取消扫描" }));

    expect(await screen.findByText("本次扫描已取消，保留上次成功结果。")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "本机开发环境" })).toBeInTheDocument();
  });

  it("项目工作区没有扫描目录时只展示加目录空态", async () => {
    render(<App />);
    await scanAndWaitForOverview();
    fireEvent.click(within(screen.getByRole("group", { name: "项目工作区" })).getByRole("button", { name: "项目" }));

    fireEvent.click(screen.getByRole("button", { name: "移除扫描目录 ~/Code" }));

    const addScanRootButton = await screen.findByRole("button", { name: /^添加扫描目录/ });
    const projectNavigation = within(screen.getByRole("navigation", { name: "项目工作区导航" }));
    expect(projectNavigation.getAllByRole("button")).toHaveLength(1);
    expect(addScanRootButton).toBeInTheDocument();
    expect(screen.getByText(/不会执行安装或修改项目/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "查看完整依赖图" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "导出 CycloneDX SBOM" })).not.toBeInTheDocument();

    // 还原浏览器预览的扫描目录状态，避免当前测试进程持续停留在无目录状态。
    fireEvent.click(addScanRootButton);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "移除扫描目录 /Users/demo/Code/new-project" })).toBeInTheDocument(),
    );
  });
});
