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
  graph: "依赖图",
  supplyChain: "供应链风险",
};

export function ProjectAnalysisPage({
  view,
  onChangeView,
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
  const [projectPath, setProjectPath] = useState(() => defaultAnalysisProjectPath(projectOptions));

  useEffect(() => {
    if (projectOptions.some((option) => option.project.path === projectPath)) return;
    setProjectPath(defaultAnalysisProjectPath(projectOptions));
  }, [projectOptions, projectPath]);

  const sharedSelection = {
    projectPath,
    onSelectProject: setProjectPath,
    showIgnoredProjects,
    onToggleIgnoredProjects: setShowIgnoredProjects,
  };

  return (
    <>
      <PageHeader
        title="项目分析"
        description="以项目为中心查看跨项目依赖索引、完整只读依赖图与供应链结构性风险；分析均离线读取本机锁文件。"
      />
      <div className="view-tabs" role="tablist" aria-label="项目分析视图">
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
      {view === "supplyChain" ? (
        <SupplyChainPage
          projects={projects}
          workspaces={workspaces}
          scanRoots={scanRoots}
          ignoredDirectoryNames={ignoredDirectoryNames}
          {...sharedSelection}
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
          onLoadGraph={onLoadGraph}
          onExportSbom={onExportSbom}
        />
      )}
    </>
  );
}
