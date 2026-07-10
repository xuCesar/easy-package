import { useState } from "react";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import { SeverityMark, StatusDot } from "../components/Status";
import { formatBytes, managerLabel } from "../lib/format";
import type { EnvironmentScan, PageId } from "../types";

export function EnvironmentPage({ data, onNavigate }: { data: EnvironmentScan; onNavigate: (page: PageId) => void }) {
  const [copied, setCopied] = useState<string>();
  const [onlyConflicts, setOnlyConflicts] = useState(false);
  const pathObservations = onlyConflicts ? data.pathObservations.filter((item) => item.hasConflict) : data.pathObservations;
  const copyText = async (id: string, value: string) => {
    await navigator.clipboard.writeText(value);
    setCopied(id);
    window.setTimeout(() => setCopied(undefined), 1200);
  };

  return (
    <>
      <PageHeader title="环境" description="检查包管理器、命令来源和 PATH 优先级。诊断信息不会自动修复。" />
      <div className="environment-layout">
        <section className="panel">
          <div className="panel__header"><h2>包管理器</h2><span className="count-label">{data.managers.length}</span></div>
          <div className="environment-managers">{data.managers.map((manager) => (
            <article className="environment-manager" key={manager.id}>
              <StatusDot status={manager.status} />
              <span className={`manager-logo manager-logo--${manager.id}`}>{manager.displayName.slice(0, 1)}</span>
              <div className="environment-manager__main"><strong>{manager.displayName}</strong><code>{manager.executablePath ?? manager.error?.message ?? "未检测到可执行文件"}</code></div>
              <div className="environment-manager__meta"><span>版本 <b>{manager.version ?? "—"}</b></span><span>缓存 <b>{formatBytes(manager.cacheSizeBytes)}</b></span></div>
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
          <div className="health-list">{data.healthIssues.map((issue) => { const destination = issue.code.includes("DEPENDENCY") || issue.code === "LOCAL_DEPENDENCY_REFERENCE" ? "dependencies" : issue.path ? "projects" : undefined; return <div className="health-item" key={issue.id}><SeverityMark severity={issue.severity} /><div><strong>{issue.title}</strong><p>{issue.description}</p>{issue.path ? <code>{issue.path}</code> : null}{issue.command ? <button className="text-button health-item__link" onClick={() => document.getElementById(`command-${issue.command}`)?.scrollIntoView({ behavior: "smooth", block: "center" })}>查看相关命令<Icon name="chevron" /></button> : destination ? <button className="text-button health-item__link" onClick={() => onNavigate(destination)}>查看相关{destination === "dependencies" ? "依赖" : "项目"}<Icon name="chevron" /></button> : null}</div></div>; })}{data.healthIssues.length === 0 ? <p className="quiet-message">环境状态良好。</p> : null}</div>
        </section>
        <section className="panel environment-logs">
          <div className="panel__header"><h2>扫描日志</h2><span className="quiet-label">最近 {data.logs.length} 条</span></div>
          <div className="log-list">{data.logs.map((log) => <div className={`log-row log-row--${log.status}`} key={log.id}><time>{new Date(log.timestamp).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" })}</time><span>{log.message}</span>{log.output ? <button className="text-button" onClick={() => void copyText(log.id, log.output ?? "")}>{copied === log.id ? "已复制" : "复制输出"}</button> : null}</div>)}</div>
        </section>
      </div>
    </>
  );
}
