import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App";

afterEach(cleanup);

describe("App", () => {
  it("加载后展示概览并支持页面导航", async () => {
    const { container } = render(<App />);
    expect(container.querySelector(".window-controls")).not.toBeInTheDocument();
    expect(screen.getByText("正在扫描本机环境")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("heading", { name: "概览" })).toBeInTheDocument());
    expect(screen.getByText(/浏览器预览：当前展示模拟数据/)).toBeInTheDocument();
    fireEvent.click(within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "软件包" }));
    expect(screen.getByRole("heading", { name: "软件包" })).toBeInTheDocument();
    fireEvent.click(within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "诊断" }));
    fireEvent.click(screen.getByRole("button", { name: "运行时" }));
    expect(screen.getByRole("heading", { name: "运行时" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "日志" }));
    expect(screen.getByRole("heading", { name: "日志" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "项目分析" }));
    expect(screen.getByRole("heading", { name: "项目分析" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "供应链风险" }));
    fireEvent.click(screen.getByRole("button", { name: "分析供应链风险" }));
    expect((await screen.findAllByText("依赖来源无法规范化")).length).toBeGreaterThan(0);
    fireEvent.click(within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "软件包" }));
    fireEvent.click(screen.getByRole("button", { name: "管理操作" }));
    expect(screen.getByRole("heading", { name: "操作中心" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "缓存清理" }));
    fireEvent.click(screen.getByRole("button", { name: "生成操作计划" }));
    expect(await screen.findByText("/opt/homebrew/bin/brew cleanup")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "系统设置" }));
    expect(screen.getByRole("heading", { name: "系统设置" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "系统扫描设置" })).toBeInTheDocument();
  });

  it("操作中心可从侧栏一级入口与「受控模式」标签直达", async () => {
    render(<App />);
    await waitFor(() => expect(screen.getByRole("heading", { name: "概览" })).toBeInTheDocument());
    fireEvent.click(within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "操作" }));
    expect(screen.getByRole("heading", { name: "操作中心" })).toBeInTheDocument();
    expect(within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "操作" })).toHaveAttribute("aria-current", "page");
    fireEvent.click(within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "概览" }));
    expect(screen.getByRole("heading", { name: "概览" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "受控模式" }));
    expect(screen.getByRole("heading", { name: "操作中心" })).toBeInTheDocument();
  });

  it("软件包页可更新筛选可一键预填批量升级计划", async () => {
    render(<App />);
    await waitFor(() => expect(screen.getByRole("heading", { name: "概览" })).toBeInTheDocument());
    fireEvent.click(within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "软件包" }));
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

  it("无可写管理器可更新包时批量升级入口禁用", async () => {
    render(<App />);
    await waitFor(() => expect(screen.getByRole("heading", { name: "概览" })).toBeInTheDocument());
    fireEvent.click(within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "软件包" }));
    fireEvent.change(screen.getByLabelText("管理器"), { target: { value: "pip" } });

    expect(screen.getByRole("button", { name: "批量生成升级计划" })).toBeDisabled();
  });

  it("软件包筛选展示空态", async () => {
    render(<App />);
    await waitFor(() => expect(screen.getByRole("heading", { name: "概览" })).toBeInTheDocument());
    fireEvent.click(within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "软件包" }));
    fireEvent.change(screen.getByPlaceholderText("搜索软件包"), { target: { value: "not-a-real-package" } });
    expect(await screen.findByText("没有匹配的软件包")).toBeInTheDocument();
  });

  it("按文本、管理器和更新状态组合筛选软件包", async () => {
    render(<App />);
    await waitFor(() => expect(screen.getByRole("heading", { name: "概览" })).toBeInTheDocument());
    fireEvent.click(within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "软件包" }));
    fireEvent.change(screen.getByPlaceholderText("搜索软件包"), { target: { value: "type" } });
    fireEvent.change(screen.getByLabelText("管理器"), { target: { value: "npm" } });
    fireEvent.change(screen.getByLabelText("状态"), { target: { value: "available" } });

    expect(screen.getByText("1 个结果")).toBeInTheDocument();
    expect(screen.getByText("typescript")).toBeInTheDocument();
    expect(screen.queryByText("git")).not.toBeInTheDocument();
  });

  it("可按 RubyGems 和 Composer 筛选只读全局包", async () => {
    render(<App />);
    await waitFor(() => expect(screen.getByRole("heading", { name: "概览" })).toBeInTheDocument());
    fireEvent.click(within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "软件包" }));

    fireEvent.change(screen.getByLabelText("管理器"), { target: { value: "rubygems" } });
    expect(screen.getByText("rake")).toBeInTheDocument();
    expect(screen.getByText("RubyGems", { selector: ".manager-chip" })).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("管理器"), { target: { value: "composer" } });
    expect(screen.getByText("psr/log")).toBeInTheDocument();
    expect(screen.getByText("Composer", { selector: ".manager-chip" })).toBeInTheDocument();
  });

  it("取消刷新后保留已有扫描结果", async () => {
    render(<App />);
    await waitFor(() => expect(screen.getByRole("heading", { name: "概览" })).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "刷新" }));
    fireEvent.click(screen.getByRole("button", { name: "取消扫描" }));

    expect(await screen.findByText("本次扫描已取消，保留上次成功结果。")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "概览" })).toBeInTheDocument();
  });
});
