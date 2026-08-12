import type { EnvironmentScan, HealthSeverity, PageId } from "../types";

export interface AttentionItem {
  id: string;
  severity: HealthSeverity;
  title: string;
  detail: string;
  target: PageId;
}

const severityRank: Record<HealthSeverity, number> = { error: 0, warning: 1, info: 2 };
const overviewHealthTargets: Record<string, PageId> = {
  MANAGER_COMMAND_FAILED: "environment",
  UNVERIFIED_EXECUTABLE: "environment",
  LARGE_CACHE: "environment",
  UPDATES_AVAILABLE: "packages",
  COMMAND_PATH_CONFLICT: "environment",
  RUNTIME_MANAGER_MISMATCH: "environment",
};

export function collectAttentionItems(data: EnvironmentScan): AttentionItem[] {
  const items: AttentionItem[] = [];

  for (const issue of data.healthIssues) {
    const target = overviewHealthTargets[issue.code];
    if (issue.severity === "info" || !target) {
      continue;
    }
    items.push({
      id: `health:${issue.id}`,
      severity: issue.severity,
      title: issue.title,
      detail: issue.description,
      target,
    });
  }

  return items.sort((a, b) => severityRank[a.severity] - severityRank[b.severity]);
}
