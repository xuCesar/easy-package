import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ProjectAnalysisPage } from "./ProjectAnalysisPage";
import type { ProjectAnalysisView, ProjectMetadata, ProjectSupplyChainReport, ProjectWorkspace } from "../types";

const projects: ProjectMetadata[] = [{
  name: "web", path: "/tmp/web", ecosystems: ["JavaScript"], lockFiles: ["package-lock.json"], runtimeRequirements: [], packageManager: "npm", dependencies: [],
  supplyChainRiskSummary: { totalCount: 2, warningCount: 1, infoCount: 1, ruleIds: ["MULTIPLE_RESOLVED_VERSIONS", "NON_REGISTRY_DEPENDENCY"] }, warnings: [],
}];

const pageProps = { workspaces: [] as ProjectWorkspace[], scanRoots: ["/tmp"], ignoredDirectoryNames: ["node_modules", ".next"] };

interface HarnessProps {
  projects: ProjectMetadata[];
  workspaces: ProjectWorkspace[];
  scanRoots: string[];
  ignoredDirectoryNames: string[];
  onLoadReport: (projectPath: string) => Promise<ProjectSupplyChainReport>;
  onExportSbom: ReturnType<typeof vi.fn>;
}

function SupplyChainHarness(props: HarnessProps) {
  const [view, setView] = useState<ProjectAnalysisView>("supplyChain");
  return <ProjectAnalysisPage view={view} onChangeView={setView} insights={[]} onLoadGraph={vi.fn()} {...props} />;
}

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
    render(<SupplyChainHarness projects={projects} {...pageProps} onLoadReport={onLoadReport} onExportSbom={vi.fn().mockResolvedValue({ saved: true })} />);
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
    render(<SupplyChainHarness projects={projects} {...pageProps} onLoadReport={vi.fn().mockResolvedValue(report)} onExportSbom={onExportSbom} />);
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
    const { rerender } = render(<SupplyChainHarness projects={projects} {...pageProps} onLoadReport={vi.fn().mockRejectedValue(new Error("锁文件读取失败"))} onExportSbom={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "分析供应链风险" }));
    expect(await screen.findByText("锁文件读取失败")).toBeInTheDocument();
    const cleanReport = { ...report, summary: { totalCount: 0, warningCount: 0, infoCount: 0, ruleIds: [] }, findings: [] };
    rerender(<SupplyChainHarness projects={projects} {...pageProps} onLoadReport={vi.fn().mockResolvedValue(cleanReport)} onExportSbom={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "分析供应链风险" }));
    expect(await screen.findByText("没有匹配的风险项")).toBeInTheDocument();
  });

  it("按工作区聚合选择项并完整统计成员风险，同时允许显式显示忽略目录", () => {
    const workspaceProjects: ProjectMetadata[] = [
      { ...projects[0], name: "easy-mes", path: "/tmp/easy-mes", supplyChainRiskSummary: { totalCount: 6, warningCount: 6, infoCount: 0, ruleIds: ["LOCKFILE_MISSING"] } },
      { ...projects[0], name: "@easy-mes/admin", path: "/tmp/easy-mes/apps/admin", supplyChainRiskSummary: { totalCount: 3, warningCount: 3, infoCount: 0, ruleIds: ["LOCKFILE_MISSING"] } },
      { ...projects[0], name: ".next", path: "/tmp/easy-mes/apps/admin/.next", supplyChainRiskSummary: { totalCount: 6, warningCount: 6, infoCount: 0, ruleIds: ["LOCKFILE_MISSING"] } },
      { ...projects[0], name: "types", path: "/tmp/easy-mes/apps/admin/.next/types", supplyChainRiskSummary: { totalCount: 6, warningCount: 6, infoCount: 0, ruleIds: ["LOCKFILE_MISSING"] } },
    ];
    render(<SupplyChainHarness projects={workspaceProjects} workspaces={[{ name: "easy-mes", path: "/tmp/easy-mes", ecosystem: "JavaScript", memberPaths: ["/tmp/easy-mes", "/tmp/easy-mes/apps/admin"] }]} scanRoots={["/tmp"]} ignoredDirectoryNames={[".next"]} onLoadReport={vi.fn()} onExportSbom={vi.fn()} />);

    expect(screen.getByText("2", { selector: ".supply-chain-metrics strong" })).toBeInTheDocument();
    expect(screen.getAllByText("9", { selector: ".supply-chain-metrics strong" })).toHaveLength(2);
    expect(screen.getByRole("option", { name: "easy-mes · 工作区 · 9 个警告" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /@easy-mes\/admin/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /\.next/ })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: "显示已忽略的生成目录" }));
    expect(screen.getByRole("option", { name: ".next · 已忽略 · 6 个警告" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "types · 已忽略 · 6 个警告" })).toBeInTheDocument();
  });
});
