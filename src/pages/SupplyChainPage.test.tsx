import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SupplyChainPage } from "./SupplyChainPage";
import type { ProjectMetadata, ProjectSupplyChainReport } from "../types";

const projects: ProjectMetadata[] = [{
  name: "web", path: "/tmp/web", ecosystems: ["JavaScript"], lockFiles: ["package-lock.json"], runtimeRequirements: [], packageManager: "npm", dependencies: [],
  supplyChainRiskSummary: { totalCount: 2, warningCount: 1, infoCount: 1, ruleIds: ["MULTIPLE_RESOLVED_VERSIONS", "NON_REGISTRY_DEPENDENCY"] }, warnings: [],
}];

const report: ProjectSupplyChainReport = {
  projectName: "web", projectPath: "/tmp/web",
  summary: { totalCount: 2, warningCount: 1, infoCount: 1, ruleIds: ["MULTIPLE_RESOLVED_VERSIONS", "NON_REGISTRY_DEPENDENCY"] },
  findings: [
    { id: "duplicate", code: "MULTIPLE_RESOLVED_VERSIONS", severity: "warning", title: "同一依赖解析为多个版本", description: "存在两个 React 版本。", projectPath: "/tmp/web", nodeId: "react", dependencyPath: ["web", "react@19.1.1"], evidence: ["JavaScript:react → 18.3.1、19.1.1"] },
    { id: "local", code: "NON_REGISTRY_DEPENDENCY", severity: "info", title: "依赖指向本地或工作区边界", description: "workspace 引用。", projectPath: "/tmp/web", nodeId: "shared", dependencyPath: ["web", "shared@workspace:*"] , evidence: ["shared workspace:*"] },
  ],
};

afterEach(cleanup);

describe("SupplyChainPage", () => {
  it("按需分析、筛选风险并展示本机证据和依赖路径", async () => {
    const onLoadReport = vi.fn().mockResolvedValue(report);
    render(<SupplyChainPage projects={projects} onLoadReport={onLoadReport} onExportSbom={vi.fn().mockResolvedValue({ saved: true })} />);
    expect(screen.getByText("尚未分析供应链风险")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "分析供应链风险" }));
    await waitFor(() => expect(onLoadReport).toHaveBeenCalledWith("/tmp/web"));
    expect(screen.getByText("同一依赖解析为多个版本")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("风险级别"), { target: { value: "info" } });
    expect(screen.getByText("依赖指向本地或工作区边界")).toBeInTheDocument();
    expect(screen.queryByText("同一依赖解析为多个版本")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /依赖指向本地或工作区边界/ }));
    expect(screen.getByText("web → shared@workspace:*")).toBeInTheDocument();
    expect(screen.getAllByText("shared workspace:*")).toHaveLength(2);
    expect(screen.getByText(/不代表已确认存在安全漏洞/)).toBeInTheDocument();
  });

  it("处理 SBOM 导出的成功、取消和失败状态", async () => {
    const onExportSbom = vi.fn().mockResolvedValueOnce({ saved: true }).mockResolvedValueOnce({ saved: false }).mockRejectedValueOnce(new Error("导出失败"));
    render(<SupplyChainPage projects={projects} onLoadReport={vi.fn().mockResolvedValue(report)} onExportSbom={onExportSbom} />);
    fireEvent.click(screen.getByRole("button", { name: "分析供应链风险" }));
    const exportButton = await screen.findByRole("button", { name: "导出含风险摘要的 SBOM" });
    fireEvent.click(exportButton);
    expect(await screen.findByText("已导出包含离线风险摘要的 CycloneDX SBOM。")).toBeInTheDocument();
    fireEvent.click(exportButton);
    expect(await screen.findByText("已取消导出。")).toBeInTheDocument();
    fireEvent.click(exportButton);
    expect(await screen.findByText("导出失败")).toBeInTheDocument();
  });

  it("展示分析失败和无风险空态", async () => {
    const { rerender } = render(<SupplyChainPage projects={projects} onLoadReport={vi.fn().mockRejectedValue(new Error("锁文件读取失败"))} onExportSbom={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "分析供应链风险" }));
    expect(await screen.findByText("锁文件读取失败")).toBeInTheDocument();
    const cleanReport = { ...report, summary: { totalCount: 0, warningCount: 0, infoCount: 0, ruleIds: [] }, findings: [] };
    rerender(<SupplyChainPage projects={projects} onLoadReport={vi.fn().mockResolvedValue(cleanReport)} onExportSbom={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "分析供应链风险" }));
    expect(await screen.findByText("没有匹配的风险项")).toBeInTheDocument();
  });
});
