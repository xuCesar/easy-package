import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mockScan } from "../mock-data";
import { EnvironmentPage } from "./EnvironmentPage";

afterEach(cleanup);

describe("EnvironmentPage", () => {
  it("展示 PATH 冲突和健康提示", () => {
    render(<EnvironmentPage data={mockScan} onNavigate={() => undefined} />);

    expect(screen.getByRole("heading", { name: "命令解析" })).toBeInTheDocument();
    expect(screen.getAllByText("多个来源")).toHaveLength(3);
    fireEvent.click(screen.getByRole("button", { name: "查看健康详情 全局环境" }));
    expect(screen.getByText("发现多个 Python 路径")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "返回健康报告" }));
    expect(screen.queryByText("环境扫描完成")).not.toBeInTheDocument();
    expect(screen.getAllByText("已识别管理器路径").length).toBeGreaterThan(0);
    expect(screen.getAllByText("已识别用户工具路径").length).toBeGreaterThan(0);
    expect(screen.getByText("3.6 GB（部分）")).toBeInTheDocument();
    expect(screen.getByText(/离线模式，不执行 registry 更新检查/)).toBeInTheDocument();
    expect(screen.getByText(/本机 SQLite 会保存扫描快照与路径元数据/)).toBeInTheDocument();
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
    data.scanRoots = ["/tmp/project"];
    data.healthIssues = [{ id: "project-warning", severity: "warning", code: "PROJECT_CONFIGURATION_MISMATCH", title: "项目配置不一致", description: "请检查锁文件", path: "/tmp/project" }];
    render(<EnvironmentPage data={data} onNavigate={onNavigate} />);
    fireEvent.click(screen.getByRole("button", { name: "查看健康详情 project" }));
    fireEvent.click(screen.getByRole("button", { name: "查看相关项目" }));
    expect(onNavigate).toHaveBeenCalledWith("projects");
  });

  it("将运行时健康项定位到运行时页", () => {
    const onNavigate = vi.fn();
    const data = structuredClone(mockScan);
    data.scanRoots = ["/tmp/project"];
    data.healthIssues = [{ id: "runtime-warning", severity: "warning", code: "RUNTIME_NOT_INSTALLED", title: "缺少 Python", description: "未发现运行时", path: "/tmp/project" }];
    render(<EnvironmentPage data={data} onNavigate={onNavigate} />);
    fireEvent.click(screen.getByRole("button", { name: "查看健康详情 project" }));
    fireEvent.click(screen.getByRole("button", { name: "查看相关运行时" }));
    expect(onNavigate).toHaveBeenCalledWith("runtimes");
  });

  it("可只显示命令冲突并展示候选来源", () => {
    const view = render(<EnvironmentPage data={mockScan} onNavigate={() => undefined} />);
    const commands = within(view.container);

    expect(commands.getAllByText("Homebrew").length).toBeGreaterThan(1);
    fireEvent.click(commands.getByRole("checkbox", { name: "仅看冲突" }));
    expect(commands.queryByText("npm", { selector: ".path-item > div > code" })).not.toBeInTheDocument();
  });

  it("按工作区聚合成员健康项并在二级页面展示详情", () => {
    const data = structuredClone(mockScan);
    data.healthIssues = [
      { id: "root", severity: "warning", code: "MULTIPLE_RESOLVED_VERSIONS", title: "根项目存在版本分歧", description: "根项目依赖分歧", path: "~/Code/easy-package" },
      { id: "member", severity: "info", code: "DEPENDENCY_CYCLE_DETECTED", title: "成员项目存在循环", description: "成员依赖循环", path: "~/Code/api-lab" },
      { id: "generated", severity: "info", code: "LOCKFILE_ENTRY_UNREACHABLE", title: "生成目录存在不可达条目", description: "旧快照生成目录提示", path: "~/Code/easy-package/.next/types" },
      { id: "executable", severity: "warning", code: "UNTRUSTED_EXECUTABLE", title: "Node 路径未经验证", description: "路径不受信任", path: "~/.nvm/versions/node/v24/bin/node" },
    ];
    render(<EnvironmentPage data={data} onNavigate={() => undefined} />);

    expect(screen.getAllByRole("button", { name: /查看健康详情/ })).toHaveLength(2);
    expect(screen.getByRole("button", { name: "查看健康详情 全局环境" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "查看健康详情 developer-tools" }));

    expect(screen.getByText("根项目存在版本分歧")).toBeInTheDocument();
    expect(screen.getByText("成员项目存在循环")).toBeInTheDocument();
    expect(screen.queryByText("生成目录存在不可达条目")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "显示已忽略项 (1)" }));
    expect(screen.getByText(/生成目录存在不可达条目/)).toBeInTheDocument();
  });

  it("保留仅含已忽略健康项的分组入口", () => {
    const data = structuredClone(mockScan);
    data.healthIssues = [{ id: "generated", severity: "info", code: "LOCKFILE_ENTRY_UNREACHABLE", title: "生成目录存在不可达条目", description: "旧快照生成目录提示", path: "~/Code/easy-package/.next/types" }];
    render(<EnvironmentPage data={data} onNavigate={() => undefined} />);

    expect(screen.getByRole("button", { name: "查看健康详情 developer-tools" })).toBeInTheDocument();
    expect(screen.getByText("1 个已忽略")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "查看健康详情 developer-tools" }));
    fireEvent.click(screen.getByRole("button", { name: "显示已忽略项 (1)" }));
    expect(screen.getByText(/生成目录存在不可达条目/)).toBeInTheDocument();
  });
});
