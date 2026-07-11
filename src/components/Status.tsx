import type { HealthSeverity, ManagerStatus, UpdateStatus } from "../types";

export function StatusDot({ status }: { status: ManagerStatus }) {
  const labels: Record<ManagerStatus, string> = { available: "可用", unavailable: "未发现", error: "错误", blocked: "已阻止", unsupported: "不支持" };
  return <span className={`status-dot status-dot--${status}`} aria-label={labels[status]} />;
}

export function UpdateBadge({ status }: { status: UpdateStatus }) {
  const labels: Record<UpdateStatus, string> = { available: "可更新", upToDate: "已是最新", unknown: "未知" };
  return <span className={`update-badge update-badge--${status}`}>{labels[status]}</span>;
}

export function SeverityMark({ severity }: { severity: HealthSeverity }) {
  return <span className={`severity-mark severity-mark--${severity}`} aria-hidden="true" />;
}
