import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App";

afterEach(cleanup);

describe("App", () => {
  it("加载后展示概览并支持页面导航", async () => {
    render(<App />);
    expect(screen.getByText("正在扫描本机环境")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("heading", { name: "本机开发环境" })).toBeInTheDocument());
    expect(screen.getByText(/浏览器预览：当前展示模拟数据/)).toBeInTheDocument();
    fireEvent.click(within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "软件包" }));
    expect(screen.getByRole("heading", { name: "软件包" })).toBeInTheDocument();
  });

  it("软件包筛选展示空态", async () => {
    render(<App />);
    await waitFor(() => expect(screen.getByRole("heading", { name: "本机开发环境" })).toBeInTheDocument());
    fireEvent.click(within(screen.getByRole("navigation", { name: "主要导航" })).getByRole("button", { name: "软件包" }));
    fireEvent.change(screen.getByPlaceholderText("搜索软件包"), { target: { value: "not-a-real-package" } });
    expect(await screen.findByText("没有匹配的软件包")).toBeInTheDocument();
  });

  it("按文本、管理器和更新状态组合筛选软件包", async () => {
    render(<App />);
    await waitFor(() => expect(screen.getByRole("heading", { name: "本机开发环境" })).toBeInTheDocument());
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
    await waitFor(() => expect(screen.getByRole("heading", { name: "本机开发环境" })).toBeInTheDocument());
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
    await waitFor(() => expect(screen.getByRole("heading", { name: "本机开发环境" })).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "刷新扫描" }));
    fireEvent.click(screen.getByRole("button", { name: "取消扫描" }));

    expect(await screen.findByText("本次扫描已取消，保留上次成功结果。")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "本机开发环境" })).toBeInTheDocument();
  });
});
