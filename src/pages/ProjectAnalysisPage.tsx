import { useEffect, useMemo, useState } from "react";
import { PageHeader } from "../components/PageHeader";
import { buildProjectAnalysisOptions, defaultAnalysisProjectPath } from "../lib/projectAnalysisOptions";
import type {
  DependencyInsight,
  ProjectAnalysisView,
  ProjectDependencyGraph,
  ProjectMetadata,
  ProjectSupplyChainReport,
  ProjectWorkspace,
  ReportExportResult,
} from "../types";
import { DependenciesPage } from "./DependenciesPage";
import { SupplyChainPage } from "./SupplyChainPage";

interface ProjectAnalysisPageProps {
  view: ProjectAnalysisView;
  onChangeView: (view: ProjectAnalysisView) => void;
  projectName?: string;
  projectPath?: string;
  onBack?: () => void;
  insights: DependencyInsight[];
  projects: ProjectMetadata[];
  workspaces: ProjectWorkspace[];
  scanRoots: string[];
  ignoredDirectoryNames: string[];
  onLoadGraph: (projectPath: string) => Promise<ProjectDependencyGraph>;
  onLoadReport: (projectPath: string) => Promise<ProjectSupplyChainReport>;
  onExportSbom: (projectPath: string) => Promise<ReportExportResult>;
}

const viewLabels: Record<ProjectAnalysisView, string> = {
  index: "依赖索引",
  graph: "完整依赖图",
  lockIssues: "锁文件问题",
};

export function ProjectAnalysisPage({
  view,
  onChangeView,
  projectName,
  projectPath: fixedProjectPath,
  onBack,
  insights,
  projects,
  workspaces,
  scanRoots,
  ignoredDirectoryNames,
  onLoadGraph,
  onLoadReport,
  onExportSbom,
}: ProjectAnalysisPageProps) {
  const [showIgnoredProjects, setShowIgnoredProjects] = useState(false);
  const projectOptions = useMemo(
    () => buildProjectAnalysisOptions(projects, workspaces, scanRoots, ignoredDirectoryNames, showIgnoredProjects),
    [ignoredDirectoryNames, projects, scanRoots, showIgnoredProjects, workspaces],
  );
  const [selectedProjectPath, setSelectedProjectPath] = useState(() => defaultAnalysisProjectPath(projectOptions));
  const projectPath = fixedProjectPath ?? selectedProjectPath;

  useEffect(() => {
    if (fixedProjectPath) return;
    if (projectOptions.some((option) => option.project.path === projectPath)) return;
    setSelectedProjectPath(defaultAnalysisProjectPath(projectOptions));
  }, [fixedProjectPath, projectOptions, projectPath]);

  const sharedSelection = {
    projectPath,
    onSelectProject: setSelectedProjectPath,
    showIgnoredProjects,
    onToggleIgnoredProjects: setShowIgnoredProjects,
  };
  const isProjectDetail = Boolean(fixedProjectPath);

  return (
    <>
      <PageHeader
        title={isProjectDetail ? `${projectName ?? "项目"} · ${viewLabels[view]}` : "项目依赖"}
        description={
          view === "lockIssues"
            ? "按需读取当前锁文件并展示结构性问题；全程离线，不查询漏洞或许可证。"
            : "按需读取当前锁文件构建完整依赖关系；不会执行包管理器命令或修改项目。"
        }
        actions={
          onBack ? (
            <button className="button button--secondary" onClick={onBack}>
              返回项目概览
            </button>
          ) : undefined
        }
      />
      {!isProjectDetail ? (
        <div className="view-tabs" role="tablist" aria-label="项目依赖视图">
          {(Object.keys(viewLabels) as ProjectAnalysisView[]).map((item) => (
            <button
              key={item}
              role="tab"
              aria-selected={view === item}
              className={view === item ? "view-tab view-tab--active" : "view-tab"}
              onClick={() => onChangeView(item)}
            >
              {viewLabels[item]}
            </button>
          ))}
        </div>
      ) : null}
      {view === "lockIssues" ? (
        <SupplyChainPage
          projects={projects}
          workspaces={workspaces}
          scanRoots={scanRoots}
          ignoredDirectoryNames={ignoredDirectoryNames}
          {...sharedSelection}
          embedded={isProjectDetail}
          onLoadReport={onLoadReport}
          onExportSbom={onExportSbom}
        />
      ) : (
        <DependenciesPage
          view={view}
          insights={insights}
          projects={projects}
          workspaces={workspaces}
          scanRoots={scanRoots}
          ignoredDirectoryNames={ignoredDirectoryNames}
          {...sharedSelection}
          embedded={isProjectDetail}
          onLoadGraph={onLoadGraph}
          onExportSbom={onExportSbom}
        />
      )}
    </>
  );
}
