import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { mockScan } from "../mock-data";
import { EnvironmentPage } from "./EnvironmentPage";

describe("EnvironmentPage", () => {
  it("展示 PATH 冲突、健康提示和扫描日志", () => {
    render(<EnvironmentPage data={mockScan} />);

    expect(screen.getByRole("heading", { name: "PATH 解析" })).toBeInTheDocument();
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

    render(<EnvironmentPage data={data} />);

    expect(screen.getByText("版本命令执行失败")).toBeInTheDocument();
    expect(screen.getByTitle("复制诊断输出")).toBeInTheDocument();
  });
});
