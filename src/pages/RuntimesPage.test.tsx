import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { RuntimesPage } from "./RuntimesPage";

const installations = [
  {
    id: "node:active",
    runtime: "Node.js",
    version: "22.17.1",
    path: "/opt/homebrew/bin/node",
    provider: "Homebrew",
    isActive: true,
    executionTrust: "managed" as const,
  },
  {
    id: "python:active",
    runtime: "Python",
    version: "3.13.5",
    path: "~/.pyenv/versions/3.13.5/bin/python",
    provider: "pyenv",
    isActive: true,
    executionTrust: "userManaged" as const,
  },
];

afterEach(cleanup);

describe("RuntimesPage", () => {
  it("仅展示本机安装来源", () => {
    render(<RuntimesPage installations={installations} />);
    expect(screen.getAllByText("Homebrew").length).toBeGreaterThan(0);
    expect(screen.getByText("用户工具路径")).toBeInTheDocument();
    expect(screen.getByText(/项目匹配结果在对应项目详情中查看/)).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "项目关联" })).not.toBeInTheDocument();
  });

  it("支持运行时和提供者筛选", () => {
    render(<RuntimesPage installations={installations} />);
    fireEvent.change(screen.getByLabelText("运行时类型"), { target: { value: "Node.js" } });
    expect(screen.getByText("22.17.1")).toBeInTheDocument();
    expect(screen.queryByText("3.13.5")).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("运行时提供者"), { target: { value: "pyenv" } });
    expect(screen.getByText("没有匹配的运行时")).toBeInTheDocument();
  });
});
