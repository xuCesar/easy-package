import { describe, expect, it } from "vitest";
import { mockScan } from "../mock-data";
import type { EnvironmentScan } from "../types";
import { collectAttentionItems } from "./attention";

const baseline: EnvironmentScan = { ...mockScan, healthIssues: [], runtimeAssessments: [], projects: [] };

describe("collectAttentionItems", () => {
  it("仅聚合全局环境问题并按严重度排序", () => {
    const data: EnvironmentScan = {
      ...baseline,
      healthIssues: [
        {
          id: "updates",
          severity: "warning",
          code: "UPDATES_AVAILABLE",
          title: "3 个软件包可更新",
          description: "详见环境页。",
        },
        {
          id: "broken",
          severity: "error",
          code: "MANAGER_COMMAND_FAILED",
          title: "npm 扫描失败",
          description: "命令退出码非零。",
        },
        {
          id: "project-lock",
          severity: "warning",
          code: "DIRECT_DEPENDENCY_NOT_RESOLVED",
          title: "api-lab 未在锁文件中解析 react",
          description: "项目依赖问题应留在项目详情。",
          path: "~/Code/api-lab",
        },
      ],
      runtimeAssessments: [
        {
          projectName: "api-lab",
          projectPath: "~/Code/api-lab",
          runtime: "Node.js",
          requirement: ">=22",
          status: "mismatch",
          activeVersion: "20.19.4",
          installedVersions: ["20.19.4"],
          message: "当前激活版本低于要求",
        },
        {
          projectName: "api-lab",
          projectPath: "~/Code/api-lab",
          runtime: "Python",
          requirement: ">=3.12",
          status: "available",
          activeVersion: "3.13.5",
          installedVersions: ["3.13.5"],
          message: "满足要求",
        },
      ],
      projects: [
        {
          ...mockScan.projects[0],
          name: "api-lab",
          path: "~/Code/api-lab",
          supplyChainRiskSummary: { totalCount: 1, warningCount: 1, infoCount: 0, ruleIds: ["LOCKFILE_MISSING"] },
        },
        {
          ...mockScan.projects[0],
          name: "clean",
          path: "~/Code/clean",
          supplyChainRiskSummary: { totalCount: 2, warningCount: 0, infoCount: 2, ruleIds: ["PACKAGE_SOURCE_UNKNOWN"] },
        },
      ],
    };

    const items = collectAttentionItems(data);

    expect(items.map((item) => item.id)).toEqual(["health:broken", "health:updates"]);
    expect(items[0].severity).toBe("error");
    expect(items[0].target).toBe("environment");
    expect(items[1].target).toBe("packages");
  });

  it("info 健康项与满足要求的运行时不计入", () => {
    const data: EnvironmentScan = {
      ...baseline,
      healthIssues: [
        {
          id: "path",
          severity: "info",
          code: "PATH_CONFLICT",
          title: "发现多个 Python 路径",
          description: "确认优先级。",
        },
      ],
    };

    expect(collectAttentionItems(data)).toEqual([]);
  });

  it("项目运行时问题不在概览展开", () => {
    const data: EnvironmentScan = {
      ...baseline,
      runtimeAssessments: [
        {
          projectName: "api-lab",
          projectPath: "~/Code/api-lab",
          runtime: "Go",
          requirement: ">=1.22",
          status: "missing",
          installedVersions: [],
          message: "未发现本机运行时",
        },
      ],
    };

    expect(collectAttentionItems(data)).toEqual([]);
  });
});
