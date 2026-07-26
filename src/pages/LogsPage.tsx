import { useMemo, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import { managerLabel } from "../lib/format";
import type { TaskLog } from "../types";

const categoryLabel: Record<TaskLog["category"], string> = {
  scan: "扫描",
  manager: "包管理器",
  project: "项目",
  storage: "存储",
};

const statusLabel: Record<TaskLog["status"], string> = {
  info: "信息",
  success: "成功",
  warning: "警告",
  error: "错误",
};

export function LogsPage({ logs }: { logs: TaskLog[] }) {
  const [category, setCategory] = useState<"all" | TaskLog["category"]>("all");
  const [status, setStatus] = useState<"all" | TaskLog["status"]>("all");
  const [copied, setCopied] = useState<string>();
  const [copyError, setCopyError] = useState<string>();
  const filteredLogs = useMemo(() => [...logs]
    .filter((log) => (category === "all" || log.category === category) && (status === "all" || log.status === status))
    .sort((left, right) => new Date(right.timestamp).getTime() - new Date(left.timestamp).getTime()), [category, logs, status]);

  const copyOutput = async (log: TaskLog) => {
    if (!log.output) return;
    try {
      await navigator.clipboard.writeText(log.output);
      setCopyError(undefined);
      setCopied(log.id);
      window.setTimeout(() => setCopied(undefined), 1200);
    } catch {
      setCopyError("无法复制诊断输出，请检查系统剪贴板权限。");
    }
  };

  return (
    <>
      <PageHeader title="日志" description="查看最近扫描的过程与诊断输出。日志仅用于查看，不会执行命令。" />
      <section className="toolbar" aria-label="日志筛选">
        <label className="select-field">类别<select aria-label="日志类别" value={category} onChange={(event) => setCategory(event.target.value as typeof category)}><option value="all">全部类别</option>{Object.entries(categoryLabel).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
        <label className="select-field">状态<select aria-label="日志状态" value={status} onChange={(event) => setStatus(event.target.value as typeof status)}><option value="all">全部状态</option>{Object.entries(statusLabel).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
        <span className="toolbar__count">{filteredLogs.length} 条日志</span>
      </section>
      {copyError ? <p className="inline-alert inline-alert--error" role="alert">{copyError}</p> : null}
      <section className="panel logs-panel">
        <div className="panel__header"><h2>扫描记录</h2><span className="quiet-label">按时间倒序</span></div>
        {filteredLogs.length ? <div className="log-list">{filteredLogs.map((log) => <article className={`log-row log-row--${log.status}`} key={log.id}>
          <time dateTime={log.timestamp}>{formatLogTime(log.timestamp)}</time>
          <div className="log-row__main"><div><span className="log-category">{categoryLabel[log.category]}</span><span className="log-status">{statusLabel[log.status]}</span>{log.managerId ? <span className="manager-chip">{managerLabel[log.managerId]}</span> : null}</div><strong>{log.message}</strong>{log.exitCode !== undefined ? <small>退出码 {log.exitCode}</small> : null}</div>
          {log.output ? <button className="text-button" onClick={() => void copyOutput(log)}>{copied === log.id ? "已复制" : "复制输出"}</button> : null}
        </article>)}</div> : <EmptyState icon="logs" title="没有匹配的日志" description="调整日志类别或状态筛选条件后重试。" />}
      </section>
    </>
  );
}

function formatLogTime(timestamp: string): string {
  return new Intl.DateTimeFormat("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit" }).format(new Date(timestamp));
}
