import type { EnvironmentScan, HealthSeverity, PageId, ProjectAnalysisView } from "../types";

export interface AttentionItem {
  id: string;
  severity: HealthSeverity;
  title: string;
  detail: string;
  target: PageId;
  analysisView?: ProjectAnalysisView;
}

const severityRank: Record<HealthSeverity, number> = { error: 0, warning: 1, info: 2 };

export function collectAttentionItems(data: EnvironmentScan): AttentionItem[] {
  const items: AttentionItem[] = [];

  for (const issue of data.healthIssues) {
    if (issue.severity === "info") {
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

  for (const assessment of data.runtimeAssessments) {
    if (assessment.status !== "mismatch" && assessment.status !== "missing") {
      continue;
    }
    items.push({
      id: `runtime:${assessment.projectPath}:${assessment.runtime}`,
      severity: assessment.status === "missing" ? "error" : "warning",
      title: `${assessment.projectName} 需要 ${assessment.runtime} ${assessment.requirement}`,
      detail: assessment.message,
      target: "runtimes",
    });
  }

  for (const project of data.projects) {
    const warningCount = project.supplyChainRiskSummary?.warningCount ?? 0;
    if (warningCount === 0) {
      continue;
    }
    items.push({
      id: `supplyChain:${project.path}`,
      severity: "warning",
      title: `${project.name} 存在 ${warningCount} 项供应链风险`,
      detail: "查看供应链风险明细并导出 SBOM。",
      target: "analysis",
      analysisView: "supplyChain",
    });
  }

  return items.sort((a, b) => severityRank[a.severity] - severityRank[b.severity]);
}
