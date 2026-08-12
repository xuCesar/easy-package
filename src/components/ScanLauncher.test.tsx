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
  });

  it("扫描状态切换为取消入口并展示进度", () => {
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
    expect(screen.getByRole("status")).toHaveTextContent("正在扫描 4/13");
  });
});
