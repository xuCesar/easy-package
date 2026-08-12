import type { EnvironmentScan, HealthSeverity, PageId } from "../types";

export interface AttentionItem {
  id: string;
  severity: HealthSeverity;
  title: string;
  detail: string;
  target: PageId;
}

const severityRank: Record<HealthSeverity, number> = { error: 0, warning: 1, info: 2 };
const overviewHealthCodes = new Set([
  "MANAGER_COMMAND_FAILED",
  "UNVERIFIED_EXECUTABLE",
  "LARGE_CACHE",
  "UPDATES_AVAILABLE",
  "COMMAND_PATH_CONFLICT",
  "RUNTIME_MANAGER_MISMATCH",
]);

export function collectAttentionItems(data: EnvironmentScan): AttentionItem[] {
  const items: AttentionItem[] = [];

  for (const issue of data.healthIssues) {
    if (issue.severity === "info" || !overviewHealthCodes.has(issue.code)) {
      continue;
    }
    items.push({
      id: `health:${issue.id}`,
      severity: issue.severity,
      title: issue.title,
      detail: issue.description,
      target: "environment",
    });
  }

  return items.sort((a, b) => severityRank[a.severity] - severityRank[b.severity]);
}
