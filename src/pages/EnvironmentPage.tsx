import { useMemo, useState } from "react";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import { SeverityMark, StatusDot } from "../components/Status";
import { formatBytes, managerLabel } from "../lib/format";
import { isIgnoredScanPath, isSameOrNestedPath, projectNameFromPath } from "../lib/projectPaths";
import type { EnvironmentScan, ExecutionTrust, HealthIssue, PageId } from "../types";

const trustLabel: Record<ExecutionTrust, string> = {
  system: "系统路径",
  managed: "已识别管理器路径",
  userManaged: "已识别用户工具路径",
  unverified: "未经验证的 PATH 路径",
  notApplicable: "未执行",
};

export function EnvironmentPage({ data, onNavigate }: { data: EnvironmentScan; onNavigate: (page: PageId) => void }) {
  const [copied, setCopied] = useState<string>();
  const [onlyConflicts, setOnlyConflicts] = useState(false);
  const [selectedHealthGroupKey, setSelectedHealthGroupKey] = useState<string>();
  const healthGroups = useMemo(() => buildHealthGroups(data), [data]);
  const selectedHealthGroup = selectedHealthGroupKey ? healthGroups.find((group) => group.key === selectedHealthGroupKey) : undefined;
  const pathObservations = onlyConflicts ? data.pathObservations.filter((item) => item.hasConflict) : data.pathObservations;
  const copyText = async (id: string, value: string) => {
    await navigator.clipboard.writeText(value);
    setCopied(id);
    window.setTimeout(() => setCopied(undefined), 1200);
  };

  if (selectedHealthGroup) return <HealthGroupDetail group={selectedHealthGroup} onBack={() => setSelectedHealthGroupKey(undefined)} onNavigate={onNavigate} />;

  return (
    <>
      <PageHeader title="环境" description={`检查包管理器、命令来源和 PATH 优先级。当前${data.scanSettings.networkPolicy === "offline" ? "为离线模式，不执行 registry 更新检查" : "允许通过本机 registry 配置读取更新状态"}。`} />
      <div className="runtime-banner"><Icon name="info" /><span>本机 SQLite 会保存扫描快照与路径元数据；导出报告会将主目录替换为 <code>~</code>，且不包含诊断原始输出。</span></div>
      <div className="environment-layout">
        <section className="panel">
          <div className="panel__header"><h2>包管理器</h2><span className="count-label">{data.managers.length}</span></div>
          <div className="environment-managers">{data.managers.map((manager) => (
            <article className="environment-manager" key={manager.id}>
              <StatusDot status={manager.status} />
              <span className={`manager-logo manager-logo--${manager.id}`}>{manager.displayName.slice(0, 1)}</span>
              <div className="environment-manager__main"><strong>{manager.displayName}</strong><code>{manager.executablePath ?? manager.error?.message ?? "未检测到可执行文件"}</code></div>
              <div className="environment-manager__meta"><span>来源 <b>{trustLabel[manager.executionTrust]}</b></span><span>版本 <b>{manager.version ?? "—"}</b></span><span>缓存 <b>{formatBytes(manager.cacheSizeBytes)}{manager.cacheScanStatus === "partial" ? "（部分）" : ""}</b></span></div>
              {manager.error?.output ? <button className="icon-button" onClick={() => void copyText(manager.id, manager.error?.output ?? "")} title="复制诊断输出"><Icon name="copy" /></button> : null}
            </article>
          ))}</div>
        </section>
        <section className="panel">
          <div className="panel__header"><h2>命令解析</h2><label className="checkbox-field"><input type="checkbox" checked={onlyConflicts} onChange={(event) => setOnlyConflicts(event.target.checked)} />仅看冲突</label></div>
          <div className="path-list">{pathObservations.map((item) => (
            <article className="path-item" id={`command-${item.command}`} key={item.command}>
              <div><code>{item.command}</code>{item.hasConflict ? <span className="conflict-label">多个来源</span> : <span className="quiet-label">当前优先项</span>}</div>
              <strong>{item.activePath ?? "未找到"}</strong>
              {item.candidates?.length ? <div className="path-candidates">{item.candidates.map((candidate) => <div key={candidate.path}><span>{candidate.pathIndex === 0 ? "当前" : `候选 ${candidate.pathIndex + 1}`}</span><code>{candidate.path}</code><small>{candidate.managerId ? managerLabel[candidate.managerId] : "未知来源"}{candidate.version ? ` · ${candidate.version}` : ""}</small></div>)}</div> : item.alternatives.length ? <p>其他路径：{item.alternatives.join(" · ")}</p> : <p>未发现其他来源</p>}
            </article>
          ))}{pathObservations.length === 0 ? <p className="quiet-message">没有命令路径冲突。</p> : null}</div>
        </section>
        <section className="panel environment-health">
          <div className="panel__header"><h2>健康报告</h2><span className="count-label">{data.healthIssues.length}</span></div>
          <div className="health-group-list">{healthGroups.map((group) => <button className="health-group" key={group.key} onClick={() => setSelectedHealthGroupKey(group.key)} aria-label={`查看健康详情 ${group.name}`}><span className="project-icon"><Icon name={group.path ? "folder" : "environment"} /></span><span className="health-group__main"><strong>{group.name}</strong><small>{group.path ?? "包管理器、缓存、PATH 与全局环境"}</small></span><span className="health-group__summary"><strong>{group.issues.length + group.ignoredIssues.length}</strong><small>{healthGroupSummary(group)}</small></span><Icon name="chevron" /></button>)}{healthGroups.length === 0 ? <p className="quiet-message">环境状态良好。</p> : null}</div>
        </section>
      </div>
    </>
  );
}

interface HealthGroup {
  key: string;
  name: string;
  path?: string;
  issues: HealthIssue[];
  ignoredIssues: HealthIssue[];
}

function HealthGroupDetail({ group, onBack, onNavigate }: { group: HealthGroup; onBack: () => void; onNavigate: (page: PageId) => void }) {
  const [showIgnored, setShowIgnored] = useState(false);
  const allIssues = [...group.issues, ...group.ignoredIssues];
  const displayedIssues = showIgnored ? [...group.issues, ...group.ignoredIssues] : group.issues;
  return <>
    <PageHeader title={`${group.name} 健康报告`} description={group.path ?? "包管理器、缓存、PATH 与全局环境健康项"} actions={<>{group.ignoredIssues.length ? <button className="button button--secondary" onClick={() => setShowIgnored((current) => !current)}>{showIgnored ? "隐藏已忽略项" : `显示已忽略项 (${group.ignoredIssues.length})`}</button> : null}<button className="button button--secondary" onClick={onBack}>返回健康报告</button></>} />
    <section className="project-detail-summary" aria-label="健康摘要"><div><span>全部健康项</span><strong>{allIssues.length}</strong></div><div><span>错误</span><strong>{allIssues.filter((issue) => issue.severity === "error").length}</strong></div><div><span>警告</span><strong>{allIssues.filter((issue) => issue.severity === "warning").length}</strong></div><div><span>提示</span><strong>{allIssues.filter((issue) => issue.severity === "info").length}</strong></div></section>
    <section className="panel health-detail-panel"><div className="panel__header"><h2>健康项详情</h2><span className="count-label">{displayedIssues.length}</span></div><div className="health-list">{displayedIssues.map((issue) => { const destination = healthDestination(issue); const ignored = group.ignoredIssues.some((item) => item.id === issue.id); return <article className={ignored ? "health-item health-item--detail health-item--ignored" : "health-item health-item--detail"} key={issue.id}><SeverityMark severity={issue.severity} /><div><strong>{issue.title}{ignored ? " · 已忽略" : ""}</strong><p>{issue.description}</p><div className="health-detail-meta"><code>{issue.code}</code>{issue.path ? <code>{issue.path}</code> : null}{issue.command ? <code>{issue.command}</code> : null}</div>{destination && !ignored ? <button className="text-button health-item__link" onClick={() => onNavigate(destination)}>查看相关{destination === "dependencies" ? "依赖" : destination === "runtimes" ? "运行时" : "项目"}<Icon name="chevron" /></button> : null}</div></article>; })}</div></section>
  </>;
}

function buildHealthGroups(data: EnvironmentScan): HealthGroup[] {
  const groups = new Map<string, HealthGroup>();
  for (const issue of data.healthIssues) {
    const owner = healthIssueOwner(issue, data);
    const group = groups.get(owner.key) ?? { ...owner, issues: [], ignoredIssues: [] };
    const ignored = issue.path ? isIgnoredScanPath(issue.path, data.scanRoots, data.scanSettings.defaultIgnoredDirectoryNames) : false;
    (ignored ? group.ignoredIssues : group.issues).push(issue);
    groups.set(owner.key, group);
  }
  return [...groups.values()].sort((left, right) => left.key === "global" ? -1 : right.key === "global" ? 1 : left.name.localeCompare(right.name));
}

function healthIssueOwner(issue: HealthIssue, data: EnvironmentScan): Pick<HealthGroup, "key" | "name" | "path"> {
  if (!issue.path || !data.scanRoots.some((root) => isSameOrNestedPath(issue.path ?? "", root))) return { key: "global", name: "全局环境" };
  const workspace = data.workspaces.filter((item) => isSameOrNestedPath(issue.path ?? "", item.path)).sort((left, right) => right.path.length - left.path.length)[0];
  if (workspace) {
    const rootProject = data.projects.find((item) => item.path === workspace.path);
    return { key: `workspace:${workspace.path}`, name: rootProject?.name ?? workspace.name, path: workspace.path };
  }
  const project = data.projects.filter((item) => isSameOrNestedPath(issue.path ?? "", item.path)).sort((left, right) => right.path.length - left.path.length)[0];
  if (project) return { key: `project:${project.path}`, name: project.name, path: project.path };
  const root = data.scanRoots.filter((item) => isSameOrNestedPath(issue.path ?? "", item)).sort((left, right) => right.length - left.length)[0];
  return { key: `root:${root}`, name: projectNameFromPath(root), path: root };
}

function severitySummary(issues: HealthIssue[]): string {
  const errors = issues.filter((issue) => issue.severity === "error").length;
  const warnings = issues.filter((issue) => issue.severity === "warning").length;
  const info = issues.length - errors - warnings;
  return [errors ? `${errors} 错误` : "", warnings ? `${warnings} 警告` : "", info ? `${info} 提示` : ""].filter(Boolean).join(" · ");
}

function healthGroupSummary(group: HealthGroup): string {
  const visibleSummary = severitySummary(group.issues);
  return [visibleSummary, group.ignoredIssues.length ? `${group.ignoredIssues.length} 个已忽略` : ""].filter(Boolean).join(" · ");
}

function healthDestination(issue: HealthIssue): PageId | undefined {
  return issue.code.startsWith("RUNTIME_") || issue.code === "ACTIVE_RUNTIME_MISMATCH" ? "runtimes" : issue.code.includes("DEPENDENCY") || issue.code === "LOCAL_DEPENDENCY_REFERENCE" ? "dependencies" : issue.path ? "projects" : undefined;
}
