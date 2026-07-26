import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import { StatusDot } from "../components/Status";
import { collectAttentionItems } from "../lib/attention";
import { formatRelativeTime } from "../lib/format";
import type { EnvironmentScan, PageId, ProjectAnalysisView, ScanProgress, SnapshotComparison } from "../types";

interface OverviewPageProps {
  data: EnvironmentScan;
  isLoading: boolean;
  scanProgress?: ScanProgress;
  comparison?: SnapshotComparison;
  onRefresh: () => void;
  onCancel: () => void;
  onNavigate: (page: PageId) => void;
  onOpenAnalysis: (view: ProjectAnalysisView) => void;
}

const phaseLabel: Record<ScanProgress["phase"], string> = {
  managers: "正在扫描包管理器",
  projects: "正在扫描项目",
  runtimes: "正在扫描运行时",
  health: "正在生成健康报告",
  complete: "扫描完成",
};

export function OverviewPage({ data, isLoading, scanProgress, comparison: _comparison, onRefresh, onCancel, onNavigate, onOpenAnalysis }: OverviewPageProps) {
  const managers = data.managers.slice(0, 4);
  const projects = data.projects.slice(0, 4);
  const scanLogs = data.logs.slice(0, 3);
  const attentionItems = collectAttentionItems(data);

  return (
    <>
      <PageHeader
        title="概览"
        description={<><span>上次扫描：{formatRelativeTime(data.scannedAt)}</span><button className="overview-header-refresh" onClick={onRefresh} disabled={isLoading}><Icon name="refresh" />{isLoading && scanProgress ? `${phaseLabel[scanProgress.phase]} ${scanProgress.completed}/${scanProgress.total}` : isLoading ? "扫描中…" : "刷新"}</button></>}
        actions={isLoading ? <button className="button button--secondary" onClick={onCancel}>取消扫描</button> : null}
      />
      <section className="overview-intro" aria-label="本机环境摘要"><h2>本机环境</h2><p>已发现 {data.managers.length} 个包管理器、{data.packages.length} 个已安装软件包与 {data.projects.length} 个项目。</p></section>
      <section className="overview-section overview-section--attention" aria-label="待关注问题">
        <div className="overview-section__header"><h2>{attentionItems.length > 0 ? `${attentionItems.length} 项需关注` : "一切正常"}</h2></div>
        <div className="overview-rows">
          {attentionItems.map((item) => (
            <button key={item.id} className="overview-row" onClick={() => item.analysisView ? onOpenAnalysis(item.analysisView) : onNavigate(item.target)}>
              <span><Icon name={item.severity === "info" ? "info" : "warning"} className={`attention-icon attention-icon--${item.severity}`} />{item.title}</span>
              <span className="overview-row__quiet">{item.detail}</span>
              <span className="overview-row__quiet"><Icon name="chevron" /></span>
            </button>
          ))}
          {attentionItems.length === 0 ? <div className="overview-row overview-row--static"><span><Icon name="check" className="attention-icon attention-icon--ok" />未发现需要关注的问题，环境状态良好。</span></div> : null}
        </div>
      </section>
      <div className="overview-simple-grid">
        <section className="overview-section"><div className="overview-section__header"><h2>已发现的管理器</h2><button className="text-button" onClick={() => onNavigate("environment")}>查看详情 <Icon name="chevron" /></button></div><div className="overview-rows">{managers.map((manager) => <button key={manager.id} className="overview-row" onClick={() => onNavigate("environment")}><span><StatusDot status={manager.status} />{manager.displayName}</span><span>{manager.version ?? "—"}</span><span>{manager.status === "available" ? "可用" : "查看状态"}</span></button>)}</div></section>
        <section className="overview-section"><div className="overview-section__header"><h2>扫描的项目</h2><button className="text-button" onClick={() => onNavigate("projects")}>查看全部 <Icon name="chevron" /></button></div><div className="overview-rows">{projects.map((project) => <button key={project.path} className="overview-row" onClick={() => onNavigate("projects")}><span>{project.name}</span><span className="overview-row__quiet">{project.ecosystems.join(" · ") || "未识别生态"}</span></button>)}{projects.length === 0 ? <button className="overview-add-project" onClick={() => onNavigate("projects")}><Icon name="plus" /><span>添加扫描目录</span></button> : null}</div></section>
      </div>
      <section className="overview-section overview-section--logs"><div className="overview-section__header"><h2>最近扫描</h2><button className="text-button" onClick={() => onNavigate("history")}>查看历史 <Icon name="chevron" /></button></div><div className="overview-rows">{scanLogs.map((log) => <div key={log.id} className="overview-row overview-row--static"><span><Icon name="history" />{log.message}</span><span className="overview-row__quiet">{formatRelativeTime(log.timestamp)}</span></div>)}{scanLogs.length === 0 ? <p className="quiet-message">尚无扫描记录。</p> : null}</div></section>
      <p className="overview-readonly"><Icon name="info" />扫描仅读取本机信息，不会修改任何文件或配置。</p>
    </>
  );
}
