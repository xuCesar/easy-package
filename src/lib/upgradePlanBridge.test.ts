import { describe, expect, it } from "vitest";
import { buildUpgradePlanPrefill, UPGRADE_PLAN_TARGET_LIMIT } from "./upgradePlanBridge";
import type { ManagedPackage } from "../types";

function pkg(overrides: Partial<ManagedPackage> & Pick<ManagedPackage, "id" | "managerId" | "name">): ManagedPackage {
  return { version: "1.0.0", latestVersion: "1.1.0", scope: "global", updateStatus: "available", ...overrides };
}

describe("buildUpgradePlanPrefill", () => {
  it("没有可写管理器的可更新包时返回 undefined", () => {
    const packages = [
      pkg({ id: "pip:black", managerId: "pip", name: "black" }),
      pkg({ id: "homebrew:git", managerId: "homebrew", name: "git", updateStatus: "upToDate" }),
    ];

    expect(buildUpgradePlanPrefill(packages)).toBeUndefined();
  });

  it("选择候选最多的可写管理器并统计被过滤的包", () => {
    const packages = [
      pkg({ id: "homebrew:git", managerId: "homebrew", name: "git" }),
      pkg({ id: "npm:typescript", managerId: "npm", name: "typescript" }),
      pkg({ id: "npm:eslint", managerId: "npm", name: "eslint" }),
      pkg({ id: "pip:black", managerId: "pip", name: "black" }),
    ];

    const prefill = buildUpgradePlanPrefill(packages);

    expect(prefill).toEqual({
      managerId: "npm",
      targets: ["typescript", "eslint"],
      truncatedCount: 0,
      otherWritableCount: 1,
      unwritableCount: 1,
    });
  });

  it("并列时按 homebrew → npm → pnpm 固定顺序取先者", () => {
    const packages = [
      pkg({ id: "npm:typescript", managerId: "npm", name: "typescript" }),
      pkg({ id: "homebrew:git", managerId: "homebrew", name: "git" }),
    ];

    expect(buildUpgradePlanPrefill(packages)?.managerId).toBe("homebrew");
  });

  it("超过 20 个目标时截断并记录数量", () => {
    const packages = Array.from({ length: UPGRADE_PLAN_TARGET_LIMIT + 3 }, (_, index) => pkg({ id: `homebrew:tool-${index}`, managerId: "homebrew", name: `tool-${index}` }));

    const prefill = buildUpgradePlanPrefill(packages);

    expect(prefill?.targets).toHaveLength(UPGRADE_PLAN_TARGET_LIMIT);
    expect(prefill?.truncatedCount).toBe(3);
  });

  it("同名包去重后只保留一个目标", () => {
    const packages = [
      pkg({ id: "homebrew:git", managerId: "homebrew", name: "git" }),
      pkg({ id: "homebrew:git-dup", managerId: "homebrew", name: "git" }),
    ];

    expect(buildUpgradePlanPrefill(packages)?.targets).toEqual(["git"]);
  });
});
