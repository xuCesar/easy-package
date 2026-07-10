import { useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import type { ProjectMetadata, ProjectWorkspace } from "../types";

interface ProjectsPageProps {
  projects: ProjectMetadata[];
  workspaces: ProjectWorkspace[];
  scanRoots: string[];
  onAddRoot: (path: string) => Promise<void>;
  onRemoveRoot: (path: string) => Promise<void>;
  onRefresh: () => void;
}

const inTauri = () => "__TAURI_INTERNALS__" in window;

function ProjectCard({ project }: { project: ProjectMetadata }) {
  return <article className="project-card" key={project.path}>
    <div className="project-card__head"><span className="project-icon"><Icon name="folder" /></span><div><h2>{project.name}</h2><p title={project.path}>{project.path}</p></div></div>
    {project.workspace ? <p className="workspace-label">{project.workspace.ecosystem} 工作区 · {project.workspace.name}</p> : <p className="workspace-label workspace-label--standalone">独立项目</p>}
    <div className="project-card__meta"><div><span>生态</span><strong>{project.ecosystems.join(" · ") || "未识别"}</strong></div><div><span>包管理器</span><strong>{project.packageManager ?? "未声明"}</strong></div></div>
    <div className="file-list">{project.lockFiles.length ? project.lockFiles.map((file) => <code key={file}>{file}</code>) : <span>未发现锁文件</span>}</div>
    {project.dependencies.length ? <div className="project-resolution-list">{project.dependencies.map((dependency) => <span key={`${dependency.ecosystem}:${dependency.normalizedName}`}><b>{dependency.name}</b><code>{dependency.versionRequirement} → {dependency.resolvedVersion ?? "未解析"}</code><small>{dependency.resolutionSource ?? "未发现对应锁文件"}</small></span>)}</div> : null}
    {project.runtimeRequirements.length ? <div className="runtime-list">{project.runtimeRequirements.map((runtime) => <span key={`${runtime.runtime}-${runtime.requirement}`}><b>{runtime.runtime}</b> {runtime.requirement}</span>)}</div> : null}
    {project.warnings.map((warning) => <p className="project-warning" key={warning}><Icon name="warning" />{warning}</p>)}
  </article>;
}

export function ProjectsPage({ projects, workspaces, scanRoots, onAddRoot, onRemoveRoot, onRefresh }: ProjectsPageProps) {
  const [isChoosing, setIsChoosing] = useState(false);
  const [actionError, setActionError] = useState<string>();
  const groups = useMemo(() => {
    const assigned = new Set(workspaces.flatMap((workspace) => workspace.memberPaths));
    return {
      workspaces: workspaces.map((workspace) => ({ workspace, projects: projects.filter((project) => workspace.memberPaths.includes(project.path)) })).filter((group) => group.projects.length > 0),
      standalone: projects.filter((project) => !assigned.has(project.path)),
    };
  }, [projects, workspaces]);

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

  return (
    <>
      <PageHeader title="项目" description="仅扫描你明确选择的目录，并读取项目类型、锁文件与运行时声明。" actions={<><button className="button button--secondary" onClick={onRefresh}><Icon name="refresh" />重新扫描</button><button className="button button--primary" onClick={() => void chooseRoot()} disabled={isChoosing}><Icon name="folder" />{isChoosing ? "选择中…" : "添加目录"}</button></>} />
      {actionError ? <div className="inline-alert inline-alert--error"><Icon name="warning" /><span>{actionError}</span></div> : null}
      {scanRoots.length ? <section className="scan-roots" aria-label="扫描目录"><span>扫描目录</span>{scanRoots.map((root) => <div key={root}><code>{root}</code><button className="icon-button icon-button--danger" onClick={() => void onRemoveRoot(root)} aria-label={`移除扫描目录 ${root}`}><Icon name="trash" /></button></div>)}</section> : null}
      {projects.length ? <div className="project-groups">{groups.workspaces.map(({ workspace, projects: members }) => <section key={workspace.path}><div className="project-group__header"><div><h2>{workspace.name}</h2><p>{workspace.ecosystem} 工作区 · {members.length} 个成员项目</p></div></div><div className="project-grid">{members.map((project) => <ProjectCard key={project.path} project={project} />)}</div></section>)}{groups.standalone.length ? <section><div className="project-group__header"><div><h2>独立项目</h2><p>未归属已识别工作区的项目</p></div></div><div className="project-grid">{groups.standalone.map((project) => <ProjectCard key={project.path} project={project} />)}</div></section> : null}</div> : <section className="panel"><EmptyState icon="folder" title="尚未添加项目目录" description="选择一个开发目录后，Easy Package 会识别其中的 JavaScript 与 Python 项目元数据。" /></section>}
    </>
  );
}
