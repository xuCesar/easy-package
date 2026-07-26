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

const assessments = [
  {
    projectName: "web",
    projectPath: "~/Code/web",
    runtime: "Node.js",
    requirement: ">=22",
    status: "available" as const,
    activeVersion: "22.17.1",
    installedVersions: ["22.17.1"],
    message: "复杂版本范围未自动判定",
  },
  {
    projectName: "worker",
    projectPath: "~/Code/worker",
    runtime: "Python",
    requirement: "3.12.0",
    status: "mismatch" as const,
    activeVersion: "3.13.5",
    installedVersions: ["3.13.5"],
    message: "当前激活版本 3.13.5 与精确声明 3.12.0 不一致",
  },
];

afterEach(cleanup);

describe("RuntimesPage", () => {
  it("展示安装来源和项目运行时关联", () => {
    render(<RuntimesPage installations={installations} assessments={assessments} />);
    expect(screen.getAllByText("Homebrew").length).toBeGreaterThan(0);
    expect(screen.getByText("用户工具路径")).toBeInTheDocument();
    expect(screen.getByText("worker")).toBeInTheDocument();
    expect(screen.getByText(/当前激活版本 3.13.5/)).toBeInTheDocument();
  });

  it("支持运行时和异常筛选", () => {
    render(<RuntimesPage installations={installations} assessments={assessments} />);
    fireEvent.change(screen.getByLabelText("运行时类型"), { target: { value: "Node.js" } });
    expect(screen.getByText("web")).toBeInTheDocument();
    expect(screen.queryByText("worker")).not.toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("仅看关联异常"));
    expect(screen.getByText("没有关联异常")).toBeInTheDocument();
  });
});
