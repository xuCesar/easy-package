import { useMemo, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import { apiErrorMessage } from "../lib/apiError";
import { formatRelativeTime } from "../lib/format";
import type {
  ReportExportResult,
  ReportFormat,
  SnapshotChangeEntity,
  SnapshotChangeKind,
  SnapshotComparison,
  SnapshotSummary,
} from "../types";

interface HistoryPageProps {
  summaries: SnapshotSummary[];
  comparison?: SnapshotComparison;
  isLoading: boolean;
  error?: string;
  onCompare: (baselineId: number, currentId: number) => Promise<SnapshotComparison>;
  onExport: (format: ReportFormat, baselineId: number, currentId: number) => Promise<ReportExportResult>;
}

const entityLabel: Record<SnapshotChangeEntity, string> = {
  manager: "包管理器",
  package: "软件包",
  project: "项目",
  health: "健康提示",
};

const kindLabel: Record<SnapshotChangeKind, string> = {
  added: "新增",
  removed: "移除",
  changed: "变化",
};

export function HistoryPage({ summaries, comparison, isLoading, error, onCompare, onExport }: HistoryPageProps) {
  const [baselineId, setBaselineId] = useState<number>();
  const [currentId, setCurrentId] = useState<number>();
  const [entity, setEntity] = useState<SnapshotChangeEntity | "all">("all");
  const [kind, setKind] = useState<SnapshotChangeKind | "all">("all");
  const [reportFormat, setReportFormat] = useState<ReportFormat>("markdown");
  const [actionError, setActionError] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const [isComparing, setIsComparing] = useState(false);
  const [isExporting, setIsExporting] = useState(false);
  const selectedBaselineId = baselineId ?? summaries[1]?.id;
  const selectedCurrentId = currentId ?? summaries[0]?.id;
  const baselineOptions = summaries.filter(
    (snapshot) => selectedCurrentId === undefined || snapshot.id < selectedCurrentId,
  );
  const currentOptions = summaries.filter(
    (snapshot) => selectedBaselineId === undefined || snapshot.id > selectedBaselineId,
  );
  const changes = useMemo(
    () =>
      comparison?.changes.filter(
        (change) => (entity === "all" || change.entity === entity) && (kind === "all" || change.kind === kind),
      ) ?? [],
    [comparison, entity, kind],
  );

  const compare = async () => {
    if (!selectedBaselineId || !selectedCurrentId || selectedBaselineId >= selectedCurrentId) {
      setActionError("基线快照必须早于当前快照。");
      return;
    }
    setIsComparing(true);
    setActionError(undefined);
    setNotice(undefined);
    try {
      await onCompare(selectedBaselineId, selectedCurrentId);
    } catch (error) {
      setActionError(apiErrorMessage(error, "历史操作失败，请重试。"));
    } finally {
      setIsComparing(false);
    }
  };

  const exportComparison = async () => {
    if (!comparison) return;
    setIsExporting(true);
    setActionError(undefined);
    setNotice(undefined);
    try {
      const result = await onExport(reportFormat, comparison.baseline.id, comparison.current.id);
      setNotice(result.saved ? "变化报告已导出。" : "已取消导出。");
    } catch (error) {
      setActionError(apiErrorMessage(error, "历史操作失败，请重试。"));
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <>
      <PageHeader title="历史" description="仅比较本机保留的最近 10 份环境快照，不读取或修改包管理器状态。" />
      {error ? (
        <div className="inline-alert inline-alert--error">
          <Icon name="warning" />
          <span>{error}</span>
        </div>
      ) : null}
      {actionError ? (
        <div className="inline-alert inline-alert--error">
          <Icon name="warning" />
          <span>{actionError}</span>
        </div>
      ) : null}
      {notice ? (
        <div className="inline-alert">
          <Icon name="info" />
          <span>{notice}</span>
        </div>
      ) : null}
      {summaries.length ? (
        <>
          <section className="metrics history-metrics" aria-label="历史摘要">
            <div className="metric">
              <span className="metric__icon">
                <Icon name="history" />
              </span>
              <div>
                <strong>{summaries.length}</strong>
                <span>保留快照</span>
              </div>
            </div>
            <div className="metric">
              <span className="metric__icon">
                <Icon name="check" />
              </span>
              <div>
                <strong>{comparison?.addedCount ?? 0}</strong>
                <span>新增项</span>
              </div>
            </div>
            <div className="metric">
              <span className="metric__icon">
                <Icon name="warning" />
              </span>
              <div>
                <strong>{comparison?.changedCount ?? 0}</strong>
                <span>变化项</span>
              </div>
            </div>
            <div className="metric">
              <span className="metric__icon">
                <Icon name="trash" />
              </span>
              <div>
                <strong>{comparison?.removedCount ?? 0}</strong>
                <span>移除项</span>
              </div>
            </div>
          </section>
          <section className="panel history-controls" aria-label="快照比较">
            <div className="panel__header">
              <h2>选择快照</h2>
              <span className="quiet-label">快照 ID 仅在本机 SQLite 内部使用</span>
            </div>
            <div className="history-controls__body">
              <label className="select-field">
                基线快照
                <select
                  aria-label="基线快照"
                  value={selectedBaselineId ?? ""}
                  onChange={(event) => {
                    const nextBaselineId = Number(event.target.value);
                    setBaselineId(nextBaselineId);
                    if (selectedCurrentId !== undefined && nextBaselineId >= selectedCurrentId)
                      setCurrentId(summaries.find((snapshot) => snapshot.id > nextBaselineId)?.id);
                  }}
                >
                  {baselineOptions.map((snapshot) => (
                    <option key={snapshot.id} value={snapshot.id}>
                      #{snapshot.id} · {formatRelativeTime(snapshot.scannedAt)}
                    </option>
                  ))}
                </select>
              </label>
              <label className="select-field">
                当前快照
                <select
                  aria-label="当前快照"
                  value={selectedCurrentId ?? ""}
                  onChange={(event) => {
                    const nextCurrentId = Number(event.target.value);
                    setCurrentId(nextCurrentId);
                    if (selectedBaselineId !== undefined && nextCurrentId <= selectedBaselineId)
                      setBaselineId(summaries.find((snapshot) => snapshot.id < nextCurrentId)?.id);
                  }}
                >
                  {currentOptions.map((snapshot) => (
                    <option key={snapshot.id} value={snapshot.id}>
                      #{snapshot.id} · {formatRelativeTime(snapshot.scannedAt)}
                    </option>
                  ))}
                </select>
              </label>
              <button
                className="button button--primary"
                onClick={() => void compare()}
                disabled={isComparing || summaries.length < 2}
              >
                {isComparing ? "比较中…" : "比较快照"}
              </button>
            </div>
          </section>
          {summaries.length === 1 ? (
            <section className="panel">
              <p className="quiet-message">至少需要两次成功扫描，才能显示环境变化。</p>
            </section>
          ) : null}
          {comparison ? (
            <>
              <div className="toolbar" role="search">
                <label className="select-field">
                  类型
                  <select
                    aria-label="变化类型"
                    value={entity}
                    onChange={(event) => setEntity(event.target.value as SnapshotChangeEntity | "all")}
                  >
                    <option value="all">全部类型</option>
                    {Object.entries(entityLabel).map(([value, label]) => (
                      <option key={value} value={value}>
                        {label}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="select-field">
                  结果
                  <select
                    aria-label="变化结果"
                    value={kind}
                    onChange={(event) => setKind(event.target.value as SnapshotChangeKind | "all")}
                  >
                    <option value="all">全部结果</option>
                    {Object.entries(kindLabel).map(([value, label]) => (
                      <option key={value} value={value}>
                        {label}
                      </option>
                    ))}
                  </select>
                </label>
                <span className="toolbar__count">{changes.length} 项变化</span>
                <label className="select-field">
                  报告
                  <select
                    aria-label="变化报告格式"
                    value={reportFormat}
                    onChange={(event) => setReportFormat(event.target.value as ReportFormat)}
                  >
                    <option value="markdown">Markdown</option>
                    <option value="json">JSON</option>
                  </select>
                </label>
                <button
                  className="button button--secondary"
                  onClick={() => void exportComparison()}
                  disabled={isExporting}
                >
                  {isExporting ? "导出中…" : "导出变化报告"}
                </button>
              </div>
              <section className="panel history-panel">
                {changes.length ? (
                  <div className="history-list">
                    {changes.map((change) => (
                      <article className="history-item" key={`${change.entity}:${change.kind}:${change.key}`}>
                        <div>
                          <span className={`history-badge history-badge--${change.kind}`}>
                            {kindLabel[change.kind]}
                          </span>
                          <span className="manager-chip">{entityLabel[change.entity]}</span>
                          <strong>{change.title}</strong>
                          <p>{change.description}</p>
                        </div>
                      </article>
                    ))}
                  </div>
                ) : (
                  <EmptyState
                    icon="history"
                    title="没有匹配的变化"
                    description="调整筛选条件，或选择其他快照进行比较。"
                  />
                )}
              </section>
            </>
          ) : null}
        </>
      ) : (
        <section className="panel">
          <EmptyState
            icon="history"
            title={isLoading ? "正在读取快照历史" : "尚无环境快照"}
            description="完成一次成功扫描后，Easy Package 会在本机保留快照以供后续比较。"
          />
        </section>
      )}
    </>
  );
}
