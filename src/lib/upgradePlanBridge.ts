import type { ManagedPackage, WritableManagerId } from "../types";

export const UPGRADE_PLAN_TARGET_LIMIT = 20;

const writableManagerOrder: readonly WritableManagerId[] = ["homebrew", "npm", "pnpm"];
const writableManagerSet = new Set<string>(writableManagerOrder);

export interface UpgradePlanPrefill {
  managerId: WritableManagerId;
  targets: string[];
  truncatedCount: number;
  otherWritableCount: number;
  unwritableCount: number;
}

export function isWritableManagerId(managerId: string): managerId is WritableManagerId {
  return writableManagerSet.has(managerId);
}

// 从当前筛选结果中挑选可更新且属于可写管理器的包；一个升级计划只能针对
// 单个管理器，因此选择候选最多的管理器（并列时按 homebrew → npm → pnpm）。
export function buildUpgradePlanPrefill(packages: ManagedPackage[]): UpgradePlanPrefill | undefined {
  const updatable = packages.filter((pkg) => pkg.updateStatus === "available");
  const unwritableCount = updatable.filter((pkg) => !isWritableManagerId(pkg.managerId)).length;
  const grouped = new Map<WritableManagerId, string[]>();
  for (const pkg of updatable) {
    if (!isWritableManagerId(pkg.managerId)) {
      continue;
    }
    const names = grouped.get(pkg.managerId) ?? [];
    if (!names.includes(pkg.name)) {
      names.push(pkg.name);
    }
    grouped.set(pkg.managerId, names);
  }

  let selected: WritableManagerId | undefined;
  for (const managerId of writableManagerOrder) {
    const count = grouped.get(managerId)?.length ?? 0;
    if (count > (selected ? (grouped.get(selected)?.length ?? 0) : 0)) {
      selected = managerId;
    }
  }
  if (!selected) {
    return undefined;
  }

  const candidates = grouped.get(selected) ?? [];
  const targets = candidates.slice(0, UPGRADE_PLAN_TARGET_LIMIT);
  const otherWritableCount = [...grouped.entries()]
    .filter(([managerId]) => managerId !== selected)
    .reduce((total, [, names]) => total + names.length, 0);

  return {
    managerId: selected,
    targets,
    truncatedCount: candidates.length - targets.length,
    otherWritableCount,
    unwritableCount,
  };
}

export function buildSingleUpgradePlanPrefill(pkg: ManagedPackage): UpgradePlanPrefill | undefined {
  return buildUpgradePlanPrefill([pkg]);
}
