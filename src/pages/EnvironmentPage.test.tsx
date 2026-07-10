import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { mockScan } from "../mock-data";
import { EnvironmentPage } from "./EnvironmentPage";

describe("EnvironmentPage", () => {
  it("展示 PATH 冲突、健康提示和扫描日志", () => {
    render(<EnvironmentPage data={mockScan} onNavigate={() => undefined} />);

    expect(screen.getByRole("heading", { name: "命令解析" })).toBeInTheDocument();
    expect(screen.getAllByText("多个来源")).toHaveLength(3);
    expect(screen.getByText("发现多个 Python 路径")).toBeInTheDocument();
    expect(screen.getByText("环境扫描完成")).toBeInTheDocument();
  });

  it("展示包管理器诊断错误", () => {
    const data = structuredClone(mockScan);
    data.managers[0] = {
      ...data.managers[0],
      status: "error",
      executablePath: undefined,
      error: { code: "VERSION_COMMAND_FAILED", message: "版本命令执行失败", output: "诊断输出" },
    };

    render(<EnvironmentPage data={data} onNavigate={() => undefined} />);

    expect(screen.getByText("版本命令执行失败")).toBeInTheDocument();
    expect(screen.getByTitle("复制诊断输出")).toBeInTheDocument();
  });

  it("将带路径的健康项定位到项目页", () => {
    const onNavigate = vi.fn();
    const data = structuredClone(mockScan);
    data.healthIssues = [{ id: "project-warning", severity: "warning", code: "PROJECT_CONFIGURATION_MISMATCH", title: "项目配置不一致", description: "请检查锁文件", path: "/tmp/project" }];
    render(<EnvironmentPage data={data} onNavigate={onNavigate} />);
    fireEvent.click(screen.getByRole("button", { name: "查看相关项目" }));
    expect(onNavigate).toHaveBeenCalledWith("projects");
  });

  it("可只显示命令冲突并展示候选来源", () => {
    const view = render(<EnvironmentPage data={mockScan} onNavigate={() => undefined} />);
    const commands = within(view.container);

    expect(commands.getAllByText("Homebrew").length).toBeGreaterThan(1);
    fireEvent.click(commands.getByRole("checkbox", { name: "仅看冲突" }));
    expect(commands.queryByText("npm", { selector: ".path-item > div > code" })).not.toBeInTheDocument();
  });
});
