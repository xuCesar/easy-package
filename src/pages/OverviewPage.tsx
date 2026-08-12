import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import { StatusDot } from "../components/Status";
import { collectAttentionItems } from "../lib/attention";
import { formatRelativeTime } from "../lib/format";
import { formatScanProgress } from "../lib/scanProgress";
import type { EnvironmentScan, PageId, ScanProgress, SnapshotComparison } from "../types";

interface OverviewPageProps {
  data: EnvironmentScan;
  isLoading: boolean;
  scanProgress?: ScanProgress;
  comparison?: SnapshotComparison;
  onRefresh: () => void;
  onCancel: () => void;
  onNavigate: (page: PageId) => void;
}

export function OverviewPage({
  data,
  isLoading,
  scanProgress,
  comparison: _comparison,
  onRefresh,
  onCancel,
  onNavigate,
}: OverviewPageProps) {
  const managers = data.managers.slice(0, 4);
  const scanLogs = data.logs.slice(0, 3);
  const attentionItems = collectAttentionItems(data);
  const availableUpdates = data.packages.filter((pkg) => pkg.updateStatus === "available").length;
  const availableManagers = data.managers.filter((manager) => manager.status === "available").length;

  return (
    <>
      <PageHeader
        title="本机开发环境"
        description={
          isLoading && scanProgress
            ? formatScanProgress(scanProgress)
            : `上次扫描：${formatRelativeTime(data.scannedAt)}`
        }
        actions={
          <>
            {isLoading ? (
              <button className="button button--secondary" onClick={onCancel}>
                取消扫描
              </button>
            ) : null}
            <button
              className="icon-button overview-refresh"
              onClick={onRefresh}
              disabled={isLoading}
              aria-label="刷新"
              title="重新扫描本机环境"
            >
              <Icon name="refresh" />
            </button>
          </>
        }
      />
      <section className="overview-summary" aria-label="本机环境摘要">
        <button className="overview-summary-card" onClick={() => onNavigate("environment")}>
          <span className="overview-summary-card__label">
            <Icon name="packages" />
            包管理器
          </span>
          <strong>{data.managers.length}</strong>
          <small>{availableManagers} 个可用</small>
        </button>
        <button className="overview-summary-card" onClick={() => onNavigate("packages")}>
          <span className="overview-summary-card__label">
            <Icon name="dependencies" />
            已安装软件包
          </span>
          <strong>{data.packages.length}</strong>
          <small>{availableUpdates > 0 ? `${availableUpdates} 个可更新` : "均为最新"}</small>
        </button>
        <button
          className="overview-summary-card"
          onClick={() => onNavigate("projects")}
          aria-label={data.projects.length === 0 ? "添加扫描目录" : "查看已扫描项目"}
        >
          <span className="overview-summary-card__label">
            <Icon name="projects" />
            已扫描项目
          </span>
          <strong>{data.projects.length}</strong>
          <small>{data.projects.length === 0 ? "添加扫描目录" : "查看项目清单"}</small>
        </button>
        <button
          className="overview-summary-card overview-summary-card--health"
          onClick={() => onNavigate("environment")}
        >
          <span className="overview-summary-card__label">
            <Icon name="environment" />
            环境健康
          </span>
          <strong>{attentionItems.length > 0 ? `${attentionItems.length} 项` : "良好"}</strong>
          <small>{attentionItems.length > 0 ? "需要进一步检查" : "未发现风险问题"}</small>
        </button>
      </section>
      <div className="overview-workspace">
        <section className="overview-section overview-section--attention" aria-label="待关注问题">
          <div className="overview-section__header">
            <h2>{attentionItems.length > 0 ? `${attentionItems.length} 项需关注` : "一切正常"}</h2>
          </div>
          <div className="overview-rows">
            {attentionItems.map((item) => (
              <button
                key={item.id}
                className="overview-row overview-row--attention"
                onClick={() => onNavigate(item.target)}
              >
                <span>
                  <Icon
                    name={item.severity === "info" ? "info" : "warning"}
                    className={`attention-icon attention-icon--${item.severity}`}
                  />
                  <span>
                    <strong>{item.title}</strong>
                    <small>{item.detail}</small>
                  </span>
                </span>
                <Icon name="chevron" />
              </button>
            ))}
            {attentionItems.length === 0 ? (
              <div className="overview-row overview-row--static">
                <span>
                  <Icon name="check" className="attention-icon attention-icon--ok" />
                  未发现需要关注的问题，环境状态良好。
                </span>
              </div>
            ) : null}
          </div>
        </section>
        <section className="overview-section">
          <div className="overview-section__header">
            <h2>管理器状态</h2>
            <button className="text-button" onClick={() => onNavigate("environment")}>
              查看详情 <Icon name="chevron" />
            </button>
          </div>
          <div className="overview-rows">
            {managers.map((manager) => (
              <button key={manager.id} className="overview-row" onClick={() => onNavigate("environment")}>
                <span>
                  <span className={`manager-logo manager-logo--${manager.id}`}>{manager.displayName.slice(0, 1)}</span>
                  {manager.displayName}
                </span>
                <span>{manager.version ?? "—"}</span>
                <span>
                  <StatusDot status={manager.status} />
                  {manager.status === "available" ? "可用" : "检查"}
                </span>
              </button>
            ))}
          </div>
        </section>
      </div>
      <section className="overview-section overview-section--logs">
        <div className="overview-section__header">
          <h2>最近扫描</h2>
          <button className="text-button" onClick={() => onNavigate("history")}>
            查看历史 <Icon name="chevron" />
          </button>
        </div>
        <div className="overview-rows">
          {scanLogs.map((log) => (
            <div key={log.id} className="overview-row overview-row--static">
              <span>
                <Icon name="history" />
                {log.message}
              </span>
              <span className="overview-row__quiet">{formatRelativeTime(log.timestamp)}</span>
            </div>
          ))}
          {scanLogs.length === 0 ? <p className="quiet-message">尚无扫描记录。</p> : null}
        </div>
      </section>
      <p className="overview-readonly">
        <Icon name="info" />
        扫描仅读取本机信息，不会修改任何文件或配置。
      </p>
    </>
  );
}
