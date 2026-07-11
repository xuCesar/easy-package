import { useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import type { ProjectMetadata, ProjectWorkspace, ReportExportResult, ReportFormat, ScanSettings } from "../types";

interface ProjectsPageProps {
  projects: ProjectMetadata[];
  workspaces: ProjectWorkspace[];
  scanRoots: string[];
  scanSettings: ScanSettings;
  onAddRoot: (path: string) => Promise<void>;
  onRemoveRoot: (path: string) => Promise<void>;
  onUpdateSettings: (settings: ScanSettings) => Promise<void>;
  onExportReport: (format: ReportFormat) => Promise<ReportExportResult>;
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

export function ProjectsPage({ projects, workspaces, scanRoots, scanSettings, onAddRoot, onRemoveRoot, onUpdateSettings, onExportReport, onRefresh }: ProjectsPageProps) {
  const [isChoosing, setIsChoosing] = useState(false);
  const [isSavingSettings, setIsSavingSettings] = useState(false);
  const [isExporting, setIsExporting] = useState(false);
  const [actionError, setActionError] = useState<string>();
  const [actionNotice, setActionNotice] = useState<string>();
  const [ignoredPath, setIgnoredPath] = useState("");
  const [maxDepth, setMaxDepth] = useState(String(scanSettings.maxDepth));
  const [reportFormat, setReportFormat] = useState<ReportFormat>("markdown");
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
    setActionNotice(undefined);
    try {
      const path = inTauri() ? await open({ directory: true, multiple: false, title: "选择项目扫描目录" }) : "/Users/demo/Code/new-project";
      if (typeof path === "string") await onAddRoot(path);
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsChoosing(false);
    }
  };

  const saveSettings = async (nextSettings: ScanSettings) => {
    setIsSavingSettings(true);
    setActionError(undefined);
    setActionNotice(undefined);
    try {
      await onUpdateSettings(nextSettings);
      setMaxDepth(String(nextSettings.maxDepth));
      setActionNotice("扫描范围已更新，项目与依赖洞察已重新计算。");
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsSavingSettings(false);
    }
  };

  const addIgnoredPath = async () => {
    const path = ignoredPath.trim();
    if (!path.startsWith("/")) {
      setActionError("请输入绝对目录路径。");
      return;
    }
    if (scanSettings.ignoredPaths.includes(path)) {
      setActionError("该目录已在忽略列表中。");
      return;
    }
    await saveSettings({ ...scanSettings, ignoredPaths: [...scanSettings.ignoredPaths, path] });
    setIgnoredPath("");
  };

  const removeIgnoredPath = async (path: string) => {
    await saveSettings({ ...scanSettings, ignoredPaths: scanSettings.ignoredPaths.filter((item) => item !== path) });
  };

  const applyMaxDepth = async () => {
    const depth = Number(maxDepth);
    if (!Number.isInteger(depth) || depth < 1 || depth > 12) {
      setActionError("最大扫描深度需在 1 到 12 之间。");
      return;
    }
    await saveSettings({ ...scanSettings, maxDepth: depth });
  };

  const exportReport = async (format: ReportFormat) => {
    setIsExporting(true);
    setActionError(undefined);
    setActionNotice(undefined);
    try {
      const result = await onExportReport(format);
      setActionNotice(result.saved ? "环境报告已导出。" : "已取消导出。");
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <>
      <PageHeader title="项目" description="仅扫描你明确选择的目录，并读取项目类型、锁文件与运行时声明。" actions={<><button className="button button--secondary" onClick={onRefresh}><Icon name="refresh" />重新扫描</button><button className="button button--primary" onClick={() => void chooseRoot()} disabled={isChoosing}><Icon name="folder" />{isChoosing ? "选择中…" : "添加目录"}</button></>} />
      {actionError ? <div className="inline-alert inline-alert--error"><Icon name="warning" /><span>{actionError}</span></div> : null}
      {actionNotice ? <div className="inline-alert"><Icon name="info" /><span>{actionNotice}</span></div> : null}
      {scanRoots.length ? <section className="scan-roots" aria-label="扫描目录"><span>扫描目录</span>{scanRoots.map((root) => <div key={root}><code>{root}</code><button className="icon-button icon-button--danger" onClick={() => void onRemoveRoot(root)} aria-label={`移除扫描目录 ${root}`}><Icon name="trash" /></button></div>)}</section> : null}
      <section className="scan-settings panel" aria-label="扫描范围设置">
        <div className="panel__header"><h2>扫描范围设置</h2><span className="quiet-label">仅影响项目遍历，不触发包管理器扫描</span></div>
        <div className="scan-settings__body">
          <div className="scan-setting"><div><strong>最大扫描深度</strong><p>默认 {scanSettings.maxDepth} 层，范围 1–12。</p></div><div className="scan-setting__control"><label className="sr-only" htmlFor="max-depth">最大扫描深度</label><input id="max-depth" type="number" min="1" max="12" value={maxDepth} onChange={(event) => setMaxDepth(event.target.value)} /><button className="button button--secondary" onClick={() => void applyMaxDepth()} disabled={isSavingSettings}>应用</button></div></div>
          <div className="scan-setting"><div><strong>默认忽略目录</strong><p>为避免遍历构建产物与依赖缓存，以下目录始终跳过。</p></div><div className="ignore-tags" aria-label="默认忽略目录">{scanSettings.defaultIgnoredDirectoryNames.map((name) => <code key={name}>{name}</code>)}</div></div>
          <div className="scan-setting"><div><strong>用户忽略目录</strong><p>仅接受位于已添加扫描目录内的现有绝对路径。</p></div><div className="ignored-paths">{scanSettings.ignoredPaths.map((path) => <span key={path}><code>{path}</code><button className="icon-button icon-button--danger" onClick={() => void removeIgnoredPath(path)} aria-label={`移除忽略目录 ${path}`} disabled={isSavingSettings}><Icon name="trash" /></button></span>)}{scanSettings.ignoredPaths.length === 0 ? <small>尚未添加用户忽略目录。</small> : null}<div className="ignored-paths__add"><label className="sr-only" htmlFor="ignored-path">忽略目录</label><input id="ignored-path" value={ignoredPath} onChange={(event) => setIgnoredPath(event.target.value)} placeholder="/Users/me/Code/archive" /><button className="button button--secondary" onClick={() => void addIgnoredPath()} disabled={isSavingSettings}>添加忽略目录</button></div></div></div>
          <div className="scan-setting"><div><strong>环境报告</strong><p>导出前会通过系统保存对话框选择位置；路径会以 <code>~</code> 脱敏，且不包含诊断原始输出。</p></div><div className="scan-setting__control"><label className="sr-only" htmlFor="report-format">报告格式</label><select id="report-format" value={reportFormat} onChange={(event) => setReportFormat(event.target.value as ReportFormat)} disabled={isExporting}><option value="markdown">Markdown</option><option value="json">JSON</option></select><button className="button button--secondary" onClick={() => void exportReport(reportFormat)} disabled={isExporting}>导出报告</button></div></div>
        </div>
      </section>
      {projects.length ? <div className="project-groups">{groups.workspaces.map(({ workspace, projects: members }) => <section key={workspace.path}><div className="project-group__header"><div><h2>{workspace.name}</h2><p>{workspace.ecosystem} 工作区 · {members.length} 个成员项目</p></div></div><div className="project-grid">{members.map((project) => <ProjectCard key={project.path} project={project} />)}</div></section>)}{groups.standalone.length ? <section><div className="project-group__header"><div><h2>独立项目</h2><p>未归属已识别工作区的项目</p></div></div><div className="project-grid">{groups.standalone.map((project) => <ProjectCard key={project.path} project={project} />)}</div></section> : null}</div> : <section className="panel"><EmptyState icon="folder" title="尚未添加项目目录" description="选择一个开发目录后，Easy Package 会识别其中的 JavaScript 与 Python 项目元数据。" /></section>}
    </>
  );
}
