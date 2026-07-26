import { describe, expect, it } from "vitest";
import { mockScan } from "../mock-data";
import { collectAttentionItems } from "./attention";
import type { EnvironmentScan } from "../types";

const baseline: EnvironmentScan = { ...mockScan, healthIssues: [], runtimeAssessments: [], projects: [] };

describe("collectAttentionItems", () => {
  it("聚合健康项、运行时不匹配与供应链警告并按严重度排序", () => {
    const data: EnvironmentScan = {
      ...baseline,
      healthIssues: [
        { id: "updates", severity: "warning", code: "UPDATES_AVAILABLE", title: "3 个软件包可更新", description: "详见环境页。" },
        { id: "broken", severity: "error", code: "MANAGER_ERROR", title: "npm 扫描失败", description: "命令退出码非零。" },
      ],
      runtimeAssessments: [
        { projectName: "api-lab", projectPath: "~/Code/api-lab", runtime: "Node.js", requirement: ">=22", status: "mismatch", activeVersion: "20.19.4", installedVersions: ["20.19.4"], message: "当前激活版本低于要求" },
        { projectName: "api-lab", projectPath: "~/Code/api-lab", runtime: "Python", requirement: ">=3.12", status: "available", activeVersion: "3.13.5", installedVersions: ["3.13.5"], message: "满足要求" },
      ],
      projects: [
        { ...mockScan.projects[0], name: "api-lab", path: "~/Code/api-lab", supplyChainRiskSummary: { totalCount: 1, warningCount: 1, infoCount: 0, ruleIds: ["LOCKFILE_MISSING"] } },
        { ...mockScan.projects[0], name: "clean", path: "~/Code/clean", supplyChainRiskSummary: { totalCount: 2, warningCount: 0, infoCount: 2, ruleIds: ["PACKAGE_SOURCE_UNKNOWN"] } },
      ],
    };

    const items = collectAttentionItems(data);

    expect(items.map((item) => item.id)).toEqual([
      "health:broken",
      "health:updates",
      "runtime:~/Code/api-lab:Node.js",
      "supplyChain:~/Code/api-lab",
    ]);
    expect(items[0].severity).toBe("error");
    expect(items[0].target).toBe("environment");
    expect(items[2].target).toBe("runtimes");
    expect(items[3].target).toBe("supplyChain");
    expect(items[3].title).toBe("api-lab 存在 1 项供应链风险");
  });

  it("info 健康项与满足要求的运行时不计入", () => {
    const data: EnvironmentScan = {
      ...baseline,
      healthIssues: [{ id: "path", severity: "info", code: "PATH_CONFLICT", title: "发现多个 Python 路径", description: "确认优先级。" }],
    };

    expect(collectAttentionItems(data)).toEqual([]);
  });

  it("缺失运行时标记为 error", () => {
    const data: EnvironmentScan = {
      ...baseline,
      runtimeAssessments: [
        { projectName: "api-lab", projectPath: "~/Code/api-lab", runtime: "Go", requirement: ">=1.22", status: "missing", installedVersions: [], message: "未发现本机运行时" },
      ],
    };

    const items = collectAttentionItems(data);
    expect(items).toHaveLength(1);
    expect(items[0].severity).toBe("error");
  });
});
