import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ScanLauncher } from "./ScanLauncher";

afterEach(cleanup);

describe("ScanLauncher", () => {
  it("使用透明线框变形结构，不再渲染带背景的应用图标", () => {
    const { container } = render(
      <ScanLauncher isInitializing={false} isScanning={false} onCancel={vi.fn()} onScan={vi.fn()} />,
    );

    expect(container.querySelector("img")).not.toBeInTheDocument();
    expect(container.querySelector(".scan-launcher__wireframe")).toBeInTheDocument();
    expect(container.querySelectorAll('animate[attributeName="points"]')).toHaveLength(3);
    expect(screen.getByText(/读取本机包管理器、全局软件包、命令来源和运行时安装/)).toBeInTheDocument();
    expect(screen.getByText(/不会修改任何文件/)).toBeInTheDocument();
  });

  it("扫描状态切换为取消入口", () => {
    const onCancel = vi.fn();
    render(
      <ScanLauncher
        isInitializing={false}
        isScanning
        progress={{ scanId: "scan-1", phase: "managers", completed: 4, total: 13 }}
        onCancel={onCancel}
        onScan={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "取消扫描" }));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it.each([
    ["managers", "正在读取包管理器 4/13"],
    ["projects", "正在查看已添加的项目目录 4/13"],
    ["runtimes", "正在读取本机运行时 4/13"],
    ["health", "正在生成健康摘要 4/13"],
    ["complete", "扫描完成"],
  ] as const)("将 %s 阶段展示为用户可读进度", (phase, expectedText) => {
    render(
      <ScanLauncher
        isInitializing={false}
        isScanning
        progress={{ scanId: "scan-1", phase, completed: 4, total: 13 }}
        onCancel={vi.fn()}
        onScan={vi.fn()}
      />,
    );

    expect(screen.getByRole("status")).toHaveTextContent(expectedText);
  });

  it("扫描失败后保留只读说明和错误提示", () => {
    render(
      <ScanLauncher error="扫描超时" isInitializing={false} isScanning={false} onCancel={vi.fn()} onScan={vi.fn()} />,
    );

    expect(screen.getByText(/不会修改任何文件/)).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("扫描超时");
  });
});
