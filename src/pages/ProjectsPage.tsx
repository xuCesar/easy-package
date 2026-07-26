import { useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import { isIgnoredScanPath, isSameOrNestedPath, projectNameFromPath } from "../lib/projectPaths";
import type { ProjectMetadata, ProjectWorkspace } from "../types";

interface ProjectsPageProps {
  projects: ProjectMetadata[];
  workspaces: ProjectWorkspace[];
  scanRoots: string[];
  ignoredDirectoryNames: string[];
  onAddRoot: (path: string) => Promise<void>;
  onRemoveRoot: (path: string) => Promise<void>;
  onRefresh: () => void;
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
  unrecognized?: boolean;
}

function ProjectListItem({ entry, onSelect }: { entry: ProjectListEntry; onSelect: () => void }) {
  return <button className="project-list-item" onClick={onSelect} aria-label={`查看项目 ${entry.name}`}>
    <span className="project-icon"><Icon name="folder" /></span>
    <span className="project-list-item__main"><strong>{entry.name}</strong><code title={entry.path}>{entry.path}</code></span>
    <span className="project-list-item__meta"><span>{entry.unrecognized ? "未识别项目类型" : entry.ecosystems.join(" · ") || "未识别生态"}</span><small>{entry.unrecognized ? "未发现项目标记" : entry.packageManager ?? "未声明包管理器"}</small></span>
    <span className="project-list-item__count"><strong>{entry.workspace ? entry.members.length : entry.dependencyCount}</strong><small>{entry.workspace ? "成员项目" : entry.unrecognized ? "扫描结果" : "直接依赖"}</small></span>
    <span className="project-list-item__workspace">{entry.workspace ? "工作区" : entry.unrecognized ? "扫描目录" : "独立项目"}</span>
    <Icon name="chevron" />
  </button>;
}

function ProjectDetail({ entry, onBack }: { entry: ProjectListEntry; onBack: () => void }) {
  const project = entry.rootProject;
  const dependencies = [project, ...entry.members].filter((item): item is ProjectMetadata => Boolean(item)).flatMap((item) => item.dependencies.map((dependency) => ({ dependency, projectName: item.name })));
  return <>
    <PageHeader title={entry.name} description={entry.path} actions={<button className="button button--secondary" onClick={onBack}>返回项目列表</button>} />
    <section className="project-detail-summary" aria-label="项目摘要">
      <div><span>生态</span><strong>{entry.ecosystems.join(" · ") || "未识别"}</strong></div>
      <div><span>包管理器</span><strong>{entry.packageManager ?? "未声明"}</strong></div>
      <div><span>{entry.workspace ? "成员项目" : "直接依赖"}</span><strong>{entry.workspace ? entry.members.length : entry.dependencyCount}</strong></div>
      <div><span>类型</span><strong>{entry.workspace ? "工作区" : entry.unrecognized ? "未识别目录" : "独立项目"}</strong></div>
    </section>
    <div className="project-detail-layout">
      <section className="panel project-detail-panel"><div className="panel__header"><h2>根项目声明</h2></div><div className="project-detail-section">{entry.unrecognized ? <p className="project-unrecognized"><Icon name="info" /><span><strong>未发现受支持的项目标记</strong><small>该扫描目录会保留在列表中；添加 package.json、pyproject.toml、Cargo.toml、go.mod 等项目文件后重新扫描即可识别。</small></span></p> : <><h3>锁文件</h3><div className="file-list">{project?.lockFiles.length ? project.lockFiles.map((file) => <code key={file}>{file}</code>) : <span>未发现锁文件</span>}</div><h3>运行时</h3><div className="runtime-list">{project?.runtimeRequirements.length ? project.runtimeRequirements.map((runtime) => <span key={`${runtime.runtime}-${runtime.requirement}`}><b>{runtime.runtime}</b> {runtime.requirement}</span>) : <span>未声明运行时</span>}</div>{project?.warnings.length ? <><h3>扫描提示</h3>{project.warnings.map((warning) => <p className="project-warning" key={warning}><Icon name="warning" />{warning}</p>)}</> : null}</>}</div></section>
      {entry.workspace ? <section className="panel project-detail-panel"><div className="panel__header"><h2>工作区成员</h2><span className="count-label">{entry.members.length}</span></div><div className="project-member-list">{entry.members.map((member) => <article key={member.path}><span className="project-icon"><Icon name="folder" /></span><div><strong>{member.name}</strong><code>{member.path}</code></div><span>{member.dependencies.length} 项依赖</span></article>)}</div></section> : null}
      <section className={entry.workspace ? "panel project-detail-panel project-detail-panel--wide" : "panel project-detail-panel"}><div className="panel__header"><h2>直接依赖</h2><span className="count-label">{dependencies.length}</span></div>{dependencies.length ? <div className="project-detail-dependencies">{dependencies.map(({ dependency, projectName }) => <article key={`${projectName}:${dependency.ecosystem}:${dependency.normalizedName}`}><div><strong>{dependency.name}</strong><span>{projectName} · {dependency.ecosystem} · {dependency.scopes.join("、")}</span></div><code>{dependency.versionRequirement} → {dependency.resolvedVersion ?? "未解析"}</code><small>{dependency.resolutionSource ?? "未发现对应锁文件"}</small></article>)}</div> : <div className="project-detail-section"><p className="quiet-message">未发现直接依赖声明。</p></div>}</section>
    </div>
  </>;
}

export function ProjectsPage({ projects, workspaces, scanRoots, ignoredDirectoryNames, onAddRoot, onRemoveRoot, onRefresh }: ProjectsPageProps) {
  const [isChoosing, setIsChoosing] = useState(false);
  const [actionError, setActionError] = useState<string>();
  const [selectedEntryId, setSelectedEntryId] = useState<string>();
  const entries = useMemo(() => buildProjectEntries(projects, workspaces, scanRoots, ignoredDirectoryNames), [ignoredDirectoryNames, projects, scanRoots, workspaces]);
  const selectedEntry = selectedEntryId ? entries.find((entry) => entry.id === selectedEntryId) : undefined;

  const chooseRoot = async () => {
    setIsChoosing(true);
    setActionError(undefined);
    try {
      const path = inTauri() ? await open({ directory: true, multiple: false, title: "选择项目扫描目录" }) : "/Users/demo/Code/new-project";
      if (typeof path === "string") await onAddRoot(path);
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsChoosing(false);
    }
  };

  if (selectedEntry) return <ProjectDetail entry={selectedEntry} onBack={() => setSelectedEntryId(undefined)} />;

  return (
    <>
      <PageHeader title="项目" description="仅扫描你明确选择的目录，并读取项目类型、锁文件与运行时声明。" actions={<><button className="button button--secondary" onClick={onRefresh}><Icon name="refresh" />重新扫描</button><button className="button button--primary" onClick={() => void chooseRoot()} disabled={isChoosing}><Icon name="folder" />{isChoosing ? "选择中…" : "添加目录"}</button></>} />
      {actionError ? <div className="inline-alert inline-alert--error"><Icon name="warning" /><span>{actionError}</span></div> : null}
      {scanRoots.length ? <section className="scan-roots" aria-label="扫描目录"><span>扫描目录</span>{scanRoots.map((root) => <div key={root}><code>{root}</code><button className="icon-button icon-button--danger" onClick={() => void onRemoveRoot(root)} aria-label={`移除扫描目录 ${root}`}><Icon name="trash" /></button></div>)}</section> : null}
      {entries.length ? <section className="panel project-list" aria-label="项目列表"><div className="panel__header"><h2>已扫描项目</h2><span className="count-label">{entries.length}</span></div>{entries.map((entry) => <ProjectListItem key={entry.id} entry={entry} onSelect={() => setSelectedEntryId(entry.id)} />)}</section> : <section className="panel project-empty"><button onClick={() => void chooseRoot()} disabled={isChoosing}><Icon name="plus" /><strong>{isChoosing ? "选择中…" : "添加扫描目录"}</strong><span>选择一个开发目录后，Easy Package 会识别项目元数据。</span></button></section>}
    </>
  );
}

function buildProjectEntries(projects: ProjectMetadata[], workspaces: ProjectWorkspace[], scanRoots: string[], ignoredDirectoryNames: string[]): ProjectListEntry[] {
  const visibleProjects = projects.filter((project) => !isIgnoredScanPath(project.path, scanRoots, ignoredDirectoryNames));
  const coveredPaths = new Set(workspaces.flatMap((workspace) => [workspace.path, ...workspace.memberPaths]));
  const workspaceEntries = workspaces.map((workspace) => {
    const rootProject = visibleProjects.find((project) => project.path === workspace.path);
    const members = visibleProjects.filter((project) => workspace.memberPaths.includes(project.path) && project.path !== workspace.path);
    const related = [rootProject, ...members].filter((project): project is ProjectMetadata => Boolean(project));
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
    } satisfies ProjectListEntry;
  });
  const standaloneEntries = visibleProjects.filter((project) => !coveredPaths.has(project.path)).map((project) => ({
    id: `project:${project.path}`,
    name: project.name,
    path: project.path,
    ecosystems: project.ecosystems,
    packageManager: project.packageManager,
    dependencyCount: project.dependencies.length,
    rootProject: project,
    members: [],
  } satisfies ProjectListEntry));
  const recognizedEntries = [...workspaceEntries, ...standaloneEntries];
  const unrecognizedRoots = scanRoots.filter((root) => !recognizedEntries.some((entry) => isSameOrNestedPath(entry.path, root))).map((root) => ({
    id: `root:${root}`,
    name: projectNameFromPath(root),
    path: root,
    ecosystems: [],
    dependencyCount: 0,
    members: [],
    unrecognized: true,
  } satisfies ProjectListEntry));
  return [...recognizedEntries, ...unrecognizedRoots];
}
