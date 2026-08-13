import { open } from "@tauri-apps/plugin-dialog";
import { useMemo, useState } from "react";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import { apiErrorMessage } from "../lib/apiError";
import { isIgnoredScanPath, isSameOrNestedPath, projectNameFromPath } from "../lib/projectPaths";
import type {
  DependencyInsight,
  ProjectDependencyGraph,
  ProjectMetadata,
  ProjectSupplyChainReport,
  ProjectWorkspace,
  ReportExportResult,
  RuntimeRequirementAssessment,
} from "../types";
import { ProjectAnalysisPage } from "./ProjectAnalysisPage";

interface ProjectsPageProps {
  projects: ProjectMetadata[];
  workspaces: ProjectWorkspace[];
  scanRoots: string[];
  dependencyInsights: DependencyInsight[];
  runtimeAssessments: RuntimeRequirementAssessment[];
  ignoredDirectoryNames: string[];
  onAddRoot: (path: string) => Promise<void>;
  onRemoveRoot: (path: string) => Promise<void>;
  onRefresh: () => void;
  onLoadGraph: (projectPath: string) => Promise<ProjectDependencyGraph>;
  onLoadReport: (projectPath: string) => Promise<ProjectSupplyChainReport>;
  onExportSbom: (projectPath: string) => Promise<ReportExportResult>;
}

const inTauri = () => "__TAURI_INTERNALS__" in window;

interface ProjectListEntry {
  id: string;
  name: string;
  path: string;
  ecosystems: string[];
  packageManager?: string;
  dependencyCount: number;
  workspace?: ProjectWorkspace;
  rootProject?: ProjectMetadata;
  members: ProjectMetadata[];
  analysisProjectPath?: string;
  unrecognized?: boolean;
}

type ProjectDetailView = "overview" | "graph" | "lockIssues";

function ProjectListItem({ entry, onSelect }: { entry: ProjectListEntry; onSelect: () => void }) {
  return (
    <button className="project-list-item" onClick={onSelect} aria-label={`查看项目 ${entry.name}`}>
      <span className="project-icon">
        <Icon name="folder" />
      </span>
      <span className="project-list-item__main">
        <strong>{entry.name}</strong>
        <code title={entry.path}>{entry.path}</code>
      </span>
      <span className="project-list-item__meta">
        <span>{entry.unrecognized ? "未识别项目类型" : entry.ecosystems.join(" · ") || "未识别生态"}</span>
        <small>{entry.unrecognized ? "未发现项目标记" : (entry.packageManager ?? "未声明包管理器")}</small>
      </span>
      <span className="project-list-item__count">
        <strong>{entry.workspace ? entry.members.length : entry.dependencyCount}</strong>
        <small>{entry.workspace ? "成员项目" : entry.unrecognized ? "扫描结果" : "直接依赖"}</small>
      </span>
      <span className="project-list-item__workspace">
        {entry.workspace ? "工作区" : entry.unrecognized ? "扫描目录" : "独立项目"}
      </span>
      <Icon name="chevron" />
    </button>
  );
}

function ProjectDetail({
  entry,
  dependencyInsights,
  runtimeAssessments,
  projects,
  workspaces,
  scanRoots,
  ignoredDirectoryNames,
  onLoadGraph,
  onLoadReport,
  onExportSbom,
  onBack,
}: {
  entry: ProjectListEntry;
  dependencyInsights: DependencyInsight[];
  runtimeAssessments: RuntimeRequirementAssessment[];
  projects: ProjectMetadata[];
  workspaces: ProjectWorkspace[];
  scanRoots: string[];
  ignoredDirectoryNames: string[];
  onLoadGraph: (projectPath: string) => Promise<ProjectDependencyGraph>;
  onLoadReport: (projectPath: string) => Promise<ProjectSupplyChainReport>;
  onExportSbom: (projectPath: string) => Promise<ReportExportResult>;
  onBack: () => void;
}) {
  const [view, setView] = useState<ProjectDetailView>("overview");
  const relatedProjects = [entry.rootProject, ...entry.members].filter((item): item is ProjectMetadata =>
    Boolean(item),
  );
  const relatedPaths = new Set(relatedProjects.map((item) => item.path));
  const dependencies = relatedProjects.flatMap((item) =>
    item.dependencies.map((dependency) => ({ dependency, projectName: item.name })),
  );
  const lockFiles = [...new Set(relatedProjects.flatMap((item) => item.lockFiles))];
  const projectAssessments = runtimeAssessments.filter((assessment) => relatedPaths.has(assessment.projectPath));
  const runtimeIssueCount = projectAssessments.filter(
    (assessment) => assessment.status === "missing" || assessment.status === "mismatch",
  ).length;
  const lockIssueCount = relatedProjects.reduce(
    (total, project) => total + (project.supplyChainRiskSummary?.totalCount ?? 0),
    0,
  );
  const analyzedLockProjectCount = relatedProjects.filter((project) => project.supplyChainRiskSummary).length;
  const lockIssueSummary = !relatedProjects.length
    ? "不适用"
    : !analyzedLockProjectCount
      ? "待分析"
      : analyzedLockProjectCount < relatedProjects.length
        ? `${lockIssueCount} · 部分待分析`
        : String(lockIssueCount);
  const lockIssuesNeedAttention =
    relatedProjects.length > 0 && (lockIssueCount > 0 || analyzedLockProjectCount < relatedProjects.length);
  const runtimeSummary = runtimeIssueCount
    ? `${runtimeIssueCount} 项需关注`
    : projectAssessments.length
      ? projectAssessments.some((assessment) => assessment.status === "unknown")
        ? "待确认"
        : "匹配"
      : relatedProjects.some((project) => project.runtimeRequirements.length)
        ? "待检查"
        : "未声明";

  if (view !== "overview" && entry.analysisProjectPath) {
    return (
      <ProjectAnalysisPage
        view={view}
        onChangeView={(nextView) => setView(nextView === "lockIssues" ? "lockIssues" : "graph")}
        projectName={entry.name}
        projectPath={entry.analysisProjectPath}
        onBack={() => setView("overview")}
        insights={dependencyInsights}
        projects={projects}
        workspaces={workspaces}
        scanRoots={scanRoots}
        ignoredDirectoryNames={ignoredDirectoryNames}
        onLoadGraph={onLoadGraph}
        onLoadReport={onLoadReport}
        onExportSbom={onExportSbom}
      />
    );
  }

  return (
    <>
      <PageHeader
        title={entry.name}
        description={entry.path}
        actions={
          <button className="button button--secondary" onClick={onBack}>
            返回项目列表
          </button>
        }
      />
      <section className="project-hub-summary" aria-label="项目概览">
        <div>
          <span>生态</span>
          <strong>{entry.ecosystems.join(" · ") || "未识别"}</strong>
        </div>
        <div>
          <span>锁文件</span>
          <strong>{lockFiles.length || "未发现"}</strong>
        </div>
        <div>
          <span>直接依赖</span>
          <strong>{entry.dependencyCount}</strong>
        </div>
        <div className={runtimeIssueCount ? "project-hub-summary__attention" : undefined}>
          <span>运行时匹配</span>
          <strong>{runtimeSummary}</strong>
        </div>
        <div className={lockIssuesNeedAttention ? "project-hub-summary__attention" : undefined}>
          <span>锁文件问题</span>
          <strong>{lockIssueSummary}</strong>
        </div>
      </section>
      {entry.analysisProjectPath ? (
        <section className="panel project-advanced-actions" aria-label="项目进阶分析">
          <div>
            <h2>进阶分析</h2>
            <p>完整图和结构问题仅在打开时读取当前锁文件，不执行安装，也不会修改项目。</p>
          </div>
          <div>
            <button className="button button--secondary" onClick={() => setView("graph")}>
              查看完整依赖图
            </button>
            <button className="button button--secondary" onClick={() => setView("lockIssues")}>
              查看锁文件问题
            </button>
          </div>
        </section>
      ) : null}
      <div className="project-detail-layout">
        <section className="panel project-detail-panel">
          <div className="panel__header">
            <h2>项目声明</h2>
          </div>
          <div className="project-detail-section">
            {entry.unrecognized ? (
              <p className="project-unrecognized">
                <Icon name="info" />
                <span>
                  <strong>未发现受支持的项目标记</strong>
                  <small>
                    该扫描目录会保留在列表中；添加 package.json、pyproject.toml、Cargo.toml、go.mod
                    等项目文件后重新扫描即可识别。
                  </small>
                </span>
              </p>
            ) : (
              <>
                <h3>锁文件</h3>
                <div className="file-list">
                  {lockFiles.length ? (
                    lockFiles.map((file) => <code key={file}>{file}</code>)
                  ) : (
                    <span>未发现锁文件</span>
                  )}
                </div>
                <h3>包管理器</h3>
                <div className="runtime-list">
                  <span>{entry.packageManager ?? "未声明包管理器"}</span>
                </div>
              </>
            )}
          </div>
        </section>
        <section className="panel project-detail-panel">
          <div className="panel__header">
            <h2>本项目运行时</h2>
            <span className="count-label">{projectAssessments.length}</span>
          </div>
          {projectAssessments.length ? (
            <div className="project-runtime-assessments">
              {projectAssessments.map((assessment) => (
                <article
                  className={`runtime-assessment runtime-assessment--${assessment.status}`}
                  key={`${assessment.projectPath}:${assessment.runtime}`}
                >
                  <div>
                    <strong>{assessment.projectName}</strong>
                    <span>
                      {assessment.runtime} {assessment.requirement}
                    </span>
                  </div>
                  <div>
                    <b>当前 {assessment.activeVersion ?? "未发现"}</b>
                    <small>{assessment.message}</small>
                  </div>
                </article>
              ))}
            </div>
          ) : (
            <div className="project-detail-section">
              <p className="quiet-message">当前项目未声明运行时，或尚无匹配结果。</p>
            </div>
          )}
        </section>
        {entry.workspace ? (
          <section className="panel project-detail-panel project-detail-panel--wide">
            <div className="panel__header">
              <h2>工作区成员</h2>
              <span className="count-label">{entry.members.length}</span>
            </div>
            <div className="project-member-list">
              {entry.members.map((member) => (
                <article key={member.path}>
                  <span className="project-icon">
                    <Icon name="folder" />
                  </span>
                  <div>
                    <strong>{member.name}</strong>
                    <code>{member.path}</code>
                  </div>
                  <span>{member.dependencies.length} 项依赖</span>
                </article>
              ))}
            </div>
          </section>
        ) : null}
        <section className="panel project-detail-panel project-detail-panel--wide">
          <div className="panel__header">
            <h2>直接依赖</h2>
            <span className="count-label">{dependencies.length}</span>
          </div>
          {dependencies.length ? (
            <div className="project-detail-dependencies">
              {dependencies.map(({ dependency, projectName }) => (
                <article key={`${projectName}:${dependency.ecosystem}:${dependency.normalizedName}`}>
                  <div>
                    <strong>{dependency.name}</strong>
                    <span>
                      {projectName} · {dependency.ecosystem} · {dependency.scopes.join("、")}
                    </span>
                  </div>
                  <code>
                    {dependency.versionRequirement} → {dependency.resolvedVersion ?? "未解析"}
                  </code>
                  <small>{dependency.resolutionSource ?? "未发现对应锁文件"}</small>
                </article>
              ))}
            </div>
          ) : (
            <div className="project-detail-section">
              <p className="quiet-message">未发现直接依赖声明。</p>
            </div>
          )}
        </section>
      </div>
    </>
  );
}

export function ProjectsPage({
  projects,
  workspaces,
  scanRoots,
  dependencyInsights,
  runtimeAssessments,
  ignoredDirectoryNames,
  onAddRoot,
  onRemoveRoot,
  onRefresh,
  onLoadGraph,
  onLoadReport,
  onExportSbom,
}: ProjectsPageProps) {
  const [isChoosing, setIsChoosing] = useState(false);
  const [actionError, setActionError] = useState<string>();
  const [selectedEntryId, setSelectedEntryId] = useState<string>();
  const entries = useMemo(
    () => buildProjectEntries(projects, workspaces, scanRoots, ignoredDirectoryNames),
    [ignoredDirectoryNames, projects, scanRoots, workspaces],
  );
  const selectedEntry = selectedEntryId ? entries.find((entry) => entry.id === selectedEntryId) : undefined;

  const chooseRoot = async () => {
    setIsChoosing(true);
    setActionError(undefined);
    try {
      const path = inTauri()
        ? await open({ directory: true, multiple: false, title: "选择项目扫描目录" })
        : "/Users/demo/Code/new-project";
      if (typeof path === "string") await onAddRoot(path);
    } catch (error) {
      setActionError(apiErrorMessage(error, "项目操作失败，请重试。"));
    } finally {
      setIsChoosing(false);
    }
  };

  if (selectedEntry)
    return (
      <ProjectDetail
        entry={selectedEntry}
        dependencyInsights={dependencyInsights}
        runtimeAssessments={runtimeAssessments}
        projects={projects}
        workspaces={workspaces}
        scanRoots={scanRoots}
        ignoredDirectoryNames={ignoredDirectoryNames}
        onLoadGraph={onLoadGraph}
        onLoadReport={onLoadReport}
        onExportSbom={onExportSbom}
        onBack={() => setSelectedEntryId(undefined)}
      />
    );

  return (
    <>
      <PageHeader
        title="项目"
        description="仅扫描你明确选择的目录，并读取项目类型、锁文件与运行时声明。"
        actions={
          <>
            <button className="button button--secondary" onClick={onRefresh}>
              <Icon name="refresh" />
              重新扫描
            </button>
            <button className="button button--primary" onClick={() => void chooseRoot()} disabled={isChoosing}>
              <Icon name="folder" />
              {isChoosing ? "选择中…" : "添加目录"}
            </button>
          </>
        }
      />
      {actionError ? (
        <div className="inline-alert inline-alert--error">
          <Icon name="warning" />
          <span>{actionError}</span>
        </div>
      ) : null}
      {scanRoots.length ? (
        <section className="scan-roots" aria-label="扫描目录">
          <span>扫描目录</span>
          {scanRoots.map((root) => (
            <div key={root}>
              <code>{root}</code>
              <button
                className="icon-button icon-button--danger"
                onClick={() => void onRemoveRoot(root)}
                aria-label={`移除扫描目录 ${root}`}
              >
                <Icon name="trash" />
              </button>
            </div>
          ))}
        </section>
      ) : null}
      {entries.length ? (
        <section className="panel project-list" aria-label="项目列表">
          <div className="panel__header">
            <h2>已扫描项目</h2>
            <span className="count-label">{entries.length}</span>
          </div>
          {entries.map((entry) => (
            <ProjectListItem key={entry.id} entry={entry} onSelect={() => setSelectedEntryId(entry.id)} />
          ))}
        </section>
      ) : (
        <section className="panel project-empty">
          <button onClick={() => void chooseRoot()} disabled={isChoosing}>
            <Icon name="plus" />
            <strong>{isChoosing ? "选择中…" : "添加扫描目录"}</strong>
            <span>只读扫描所选路径内的项目声明和锁文件；不会执行安装或修改项目。</span>
          </button>
        </section>
      )}
    </>
  );
}

function buildProjectEntries(
  projects: ProjectMetadata[],
  workspaces: ProjectWorkspace[],
  scanRoots: string[],
  ignoredDirectoryNames: string[],
): ProjectListEntry[] {
  const visibleProjects = projects.filter(
    (project) => !isIgnoredScanPath(project.path, scanRoots, ignoredDirectoryNames),
  );
  const coveredPaths = new Set(workspaces.flatMap((workspace) => [workspace.path, ...workspace.memberPaths]));
  const workspaceEntries = workspaces.map((workspace) => {
    const rootProject = visibleProjects.find((project) => project.path === workspace.path);
    const members = visibleProjects.filter(
      (project) => workspace.memberPaths.includes(project.path) && project.path !== workspace.path,
    );
    const related = [rootProject, ...members].filter((project): project is ProjectMetadata => Boolean(project));
    const analysisProject = preferredAnalysisProject(related);
    return {
      id: `workspace:${workspace.path}`,
      name: rootProject?.name ?? workspace.name,
      path: workspace.path,
      ecosystems: [...new Set(related.flatMap((project) => project.ecosystems))],
      packageManager: rootProject?.packageManager ?? `${workspace.ecosystem} workspace`,
      dependencyCount: related.reduce((total, project) => total + project.dependencies.length, 0),
      workspace,
      rootProject,
      members,
      analysisProjectPath: analysisProject?.path,
    } satisfies ProjectListEntry;
  });
  const standaloneEntries = visibleProjects
    .filter((project) => !coveredPaths.has(project.path))
    .map(
      (project) =>
        ({
          id: `project:${project.path}`,
          name: project.name,
          path: project.path,
          ecosystems: project.ecosystems,
          packageManager: project.packageManager,
          dependencyCount: project.dependencies.length,
          rootProject: project,
          members: [],
          analysisProjectPath: project.path,
        }) satisfies ProjectListEntry,
    );
  const recognizedEntries = [...workspaceEntries, ...standaloneEntries];
  const unrecognizedRoots = scanRoots
    .filter((root) => !recognizedEntries.some((entry) => isSameOrNestedPath(entry.path, root)))
    .map(
      (root) =>
        ({
          id: `root:${root}`,
          name: projectNameFromPath(root),
          path: root,
          ecosystems: [],
          dependencyCount: 0,
          members: [],
          unrecognized: true,
        }) satisfies ProjectListEntry,
    );
  return [...recognizedEntries, ...unrecognizedRoots];
}

function preferredAnalysisProject(projects: ProjectMetadata[]): ProjectMetadata | undefined {
  return (
    projects.find((project) => project.dependencyGraphSummary) ??
    projects.find((project) => project.supplyChainRiskSummary) ??
    projects[0]
  );
}
