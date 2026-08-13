import type { ProjectMetadata, ProjectWorkspace } from "../types";
import { isIgnoredScanPath } from "./projectPaths";

export interface ProjectAnalysisOption {
  name: string;
  project: ProjectMetadata;
  projects: ProjectMetadata[];
  supplyChainRiskSummary?: ProjectMetadata["supplyChainRiskSummary"];
  isIgnored: boolean;
  isWorkspace: boolean;
}

/**
 * 将扫描结果转换为分析页的一级对象。工作区成员仍保留在快照中，
 * 但不应在项目级分析入口中与工作区根重复出现。
 */
export function buildProjectAnalysisOptions(
  projects: ProjectMetadata[],
  workspaces: ProjectWorkspace[],
  scanRoots: string[],
  ignoredDirectoryNames: string[],
  showIgnoredProjects: boolean,
): ProjectAnalysisOption[] {
  const projectByPath = new Map(projects.map((project) => [project.path, project]));
  const workspaceMemberPaths = new Set(workspaces.flatMap((workspace) => [workspace.path, ...workspace.memberPaths]));
  const workspaceOptions = workspaces.flatMap((workspace) => {
    const relatedProjects = [...new Set([workspace.path, ...workspace.memberPaths])]
      .map((path) => projectByPath.get(path))
      .filter((project): project is ProjectMetadata => Boolean(project))
      .filter((project) => showIgnoredProjects || !isIgnoredScanPath(project.path, scanRoots, ignoredDirectoryNames));
    const rootProject = relatedProjects.find((project) => project.path === workspace.path);
    const representativeProject = rootProject ?? relatedProjects[0];
    if (!representativeProject) return [];
    return [
      {
        name: rootProject?.name ?? workspace.name,
        project: representativeProject,
        projects: relatedProjects,
        supplyChainRiskSummary: aggregateSupplyChainRiskSummaries(relatedProjects),
        isIgnored: isIgnoredScanPath(representativeProject.path, scanRoots, ignoredDirectoryNames),
        isWorkspace: true,
      },
    ];
  });
  const standaloneOptions = projects
    .filter((project) => !workspaceMemberPaths.has(project.path))
    .map((project) => ({
      name: project.name,
      project,
      projects: [project],
      supplyChainRiskSummary: project.supplyChainRiskSummary,
      isIgnored: isIgnoredScanPath(project.path, scanRoots, ignoredDirectoryNames),
      isWorkspace: false,
    }))
    .filter((option) => showIgnoredProjects || !option.isIgnored);

  return [...workspaceOptions, ...standaloneOptions].sort((left, right) => left.name.localeCompare(right.name));
}

/** 统一的默认选中项：优先有依赖图摘要的项目，其次有锁文件问题摘要的，最后取首个。 */
export function defaultAnalysisProjectPath(options: ProjectAnalysisOption[]): string {
  return (
    options.find((option) => option.project.dependencyGraphSummary)?.project.path ??
    options.find((option) => option.supplyChainRiskSummary)?.project.path ??
    options[0]?.project.path ??
    ""
  );
}

function aggregateSupplyChainRiskSummaries(projects: ProjectMetadata[]): ProjectMetadata["supplyChainRiskSummary"] {
  const summaries = projects.flatMap((project) =>
    project.supplyChainRiskSummary ? [project.supplyChainRiskSummary] : [],
  );
  if (!summaries.length) return undefined;
  return {
    totalCount: summaries.reduce((total, summary) => total + summary.totalCount, 0),
    warningCount: summaries.reduce((total, summary) => total + summary.warningCount, 0),
    infoCount: summaries.reduce((total, summary) => total + summary.infoCount, 0),
    ruleIds: [...new Set(summaries.flatMap((summary) => summary.ruleIds))].sort(),
  };
}
