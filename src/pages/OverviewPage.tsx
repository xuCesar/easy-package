import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import { SeverityMark, StatusDot, UpdateBadge } from "../components/Status";
import { formatBytes, formatRelativeTime } from "../lib/format";
import type { EnvironmentScan, PageId, ScanProgress } from "../types";

interface OverviewPageProps {
  data: EnvironmentScan;
  isLoading: boolean;
  scanProgress?: ScanProgress;
  onRefresh: () => void;
  onCancel: () => void;
  onNavigate: (page: PageId) => void;
}

const phaseLabel: Record<ScanProgress["phase"], string> = {
  managers: "正在扫描包管理器",
  projects: "正在扫描项目",
  health: "正在生成健康报告",
  complete: "扫描完成",
};

export function OverviewPage({ data, isLoading, scanProgress, onRefresh, onCancel, onNavigate }: OverviewPageProps) {
  const availableManagers = data.managers.filter((manager) => manager.status === "available").length;
  const outdated = data.packages.filter((pkg) => pkg.updateStatus === "available").length;
  const cacheSize = data.managers.reduce((total, manager) => total + (manager.cacheSizeBytes ?? 0), 0);
  const divergentDependencies = data.dependencyInsights.filter((insight) => insight.hasVersionDivergence).length;
  const dependencyRisks = data.dependencyInsights.filter((insight) => insight.hasHealthRisk).length;

  return (
    <>
      <PageHeader
        title="本机开发环境"
        description={`上次扫描：${formatRelativeTime(data.scannedAt)} · 所有数据仅保存在本机`}
        actions={<>{isLoading ? <button className="button button--secondary" onClick={onCancel}>取消扫描</button> : null}<button className="button button--primary" onClick={onRefresh} disabled={isLoading}><Icon name="refresh" />{isLoading && scanProgress ? `${phaseLabel[scanProgress.phase]} ${scanProgress.completed}/${scanProgress.total}` : isLoading ? "扫描中…" : "刷新扫描"}</button></>}
      />
      <section className="metrics" aria-label="环境摘要">
        <div className="metric"><span className="metric__icon"><Icon name="packages" /></span><div><strong>{availableManagers}</strong><span>已发现管理器</span></div></div>
        <div className="metric"><span className="metric__icon"><Icon name="refresh" /></span><div><strong>{outdated}</strong><span>可更新软件包</span></div></div>
        <div className="metric"><span className="metric__icon"><Icon name="environment" /></span><div><strong>{formatBytes(cacheSize)}</strong><span>缓存占用</span></div></div>
        <div className="metric"><span className="metric__icon"><Icon name="info" /></span><div><strong>{data.healthIssues.length}</strong><span>健康提示</span></div></div>
      </section>
      <div className="overview-grid">
        <section className="panel panel--managers">
          <div className="panel__header"><h2>包管理器</h2><button className="text-button" onClick={() => onNavigate("environment")}>查看环境 <Icon name="chevron" /></button></div>
          <div className="manager-list">
            {data.managers.map((manager) => (
              <button className="manager-row" key={manager.id} onClick={() => onNavigate("environment")}>
                <StatusDot status={manager.status} />
                <span className={`manager-logo manager-logo--${manager.id}`}>{manager.displayName.slice(0, 1)}</span>
                <span className="manager-row__name"><strong>{manager.displayName}</strong><small>{manager.executablePath ?? manager.error?.message ?? "未检测到"}</small></span>
                <span className="manager-row__version">{manager.version ?? "—"}</span>
                <Icon name="chevron" />
              </button>
            ))}
          </div>
        </section>
        <section className="panel panel--health">
          <div className="panel__header"><h2>健康提示</h2><span className="count-label">{data.healthIssues.length}</span></div>
          <div className="health-list">
            {data.healthIssues.slice(0, 3).map((issue) => (
              <div className="health-item" key={issue.id}>
                <SeverityMark severity={issue.severity} />
                <div><strong>{issue.title}</strong><p>{issue.description}</p></div>
              </div>
            ))}
            {data.healthIssues.length === 0 ? <p className="quiet-message">未发现需要关注的问题。</p> : null}
          </div>
          <button className="text-button panel__link" onClick={() => onNavigate("environment")}>查看完整报告 <Icon name="chevron" /></button>
        </section>
        <section className="panel panel--dependencies">
          <div className="panel__header"><h2>依赖洞察</h2><button className="text-button" onClick={() => onNavigate("dependencies")}>查看全部 <Icon name="chevron" /></button></div>
          <div className="dependency-preview"><strong>{data.dependencyInsights.length}</strong><span>项直接依赖</span><p>{dependencyRisks ? `${dependencyRisks} 项依赖需要关注${divergentDependencies ? `，含 ${divergentDependencies} 项版本分歧` : ""}` : "未发现跨项目依赖风险"}</p></div>
        </section>
        <section className="panel panel--updates">
          <div className="panel__header"><h2>软件包状态</h2><button className="text-button" onClick={() => onNavigate("packages")}>查看全部 <Icon name="chevron" /></button></div>
          <div className="table-wrap">
            <table>
              <thead><tr><th>软件包</th><th>管理器</th><th>已安装</th><th>最新版本</th><th>状态</th></tr></thead>
              <tbody>{data.packages.slice(0, 6).map((pkg) => <tr key={pkg.id}><td><strong>{pkg.name}</strong></td><td>{pkg.managerId}</td><td className="mono">{pkg.version}</td><td className="mono">{pkg.latestVersion ?? "—"}</td><td><UpdateBadge status={pkg.updateStatus} /></td></tr>)}</tbody>
            </table>
          </div>
        </section>
      </div>
    </>
  );
}
