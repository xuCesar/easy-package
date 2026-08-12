import { memo, type ReactNode, useCallback, useState } from "react";
import { isTauriRuntime } from "./api";
import { AppShell } from "./components/AppShell";
import { Icon } from "./components/Icon";
import { ScanLauncher } from "./components/ScanLauncher";
import { useDevPkg } from "./hooks/useDevPkg";
import { useSnapshotHistory } from "./hooks/useSnapshotHistory";
import type { UpgradePlanPrefill } from "./lib/upgradePlanBridge";
import { ActionCenterContainer } from "./pages/ActionCenterContainer";
import { EnvironmentPage } from "./pages/EnvironmentPage";
import { HistoryPage } from "./pages/HistoryPage";
import { LogsPage } from "./pages/LogsPage";
import { OverviewPage } from "./pages/OverviewPage";
import { PackagesPage } from "./pages/PackagesPage";
import { ProjectAnalysisPage } from "./pages/ProjectAnalysisPage";
import { ProjectsPage } from "./pages/ProjectsPage";
import { RuntimesPage } from "./pages/RuntimesPage";
import { SettingsPage } from "./pages/SettingsPage";
import type { PageId, ProjectAnalysisView } from "./types";

const diagnosticPages = ["environment", "analysis", "runtimes", "history", "logs"] as const satisfies readonly PageId[];
const diagnosticLabels: Record<(typeof diagnosticPages)[number], string> = {
  environment: "环境",
  analysis: "项目分析",
  runtimes: "运行时",
  history: "历史",
  logs: "日志",
};
const diagnosticPageSet = new Set<PageId>(diagnosticPages);

// 扫描进度高频更新只应重渲染消费它的概览页；其余页面通过 memo + 稳定 props 跳过。
const MemoPackagesPage = memo(PackagesPage);
const MemoProjectsPage = memo(ProjectsPage);
const MemoProjectAnalysisPage = memo(ProjectAnalysisPage);
const MemoRuntimesPage = memo(RuntimesPage);
const MemoHistoryPage = memo(HistoryPage);
const MemoEnvironmentPage = memo(EnvironmentPage);
const MemoLogsPage = memo(LogsPage);
const MemoSettingsPage = memo(SettingsPage);

export function App() {
  const [page, setPage] = useState<PageId>("overview");
  const [upgradePrefill, setUpgradePrefill] = useState<UpgradePlanPrefill>();
  const [analysisView, setAnalysisView] = useState<ProjectAnalysisView>("index");
  const navigate = useCallback((next: PageId) => {
    setUpgradePrefill(undefined);
    setPage(next);
  }, []);
  const startUpgradePlan = useCallback((prefill: UpgradePlanPrefill) => {
    setUpgradePrefill(prefill);
    setPage("actions");
  }, []);
  const {
    data,
    isInitializing,
    isLoading,
    error,
    scanProgress,
    notice,
    refresh,
    cancelScan,
    addRoot,
    removeRoot,
    updateScanSettings,
    exportEnvironmentReport,
    getProjectDependencyGraph,
    getProjectSupplyChainReport,
    exportProjectSbom,
    applyEnvironment,
  } = useDevPkg();
  const history = useSnapshotHistory(data?.scannedAt);
  const refreshNow = useCallback(() => void refresh(), [refresh]);
  const cancelScanNow = useCallback(() => void cancelScan(), [cancelScan]);

  let content: ReactNode;
  if (!data) {
    content = (
      <ScanLauncher
        error={error}
        isInitializing={isInitializing}
        isScanning={isLoading}
        progress={scanProgress}
        onCancel={() => void cancelScan()}
        onScan={() => void refresh()}
      />
    );
  } else {
    content = (
      <>
        {error ? (
          <div className="inline-alert inline-alert--error">
            <Icon name="warning" />
            <span>{error}</span>
            <button onClick={() => void refresh()}>重试</button>
          </div>
        ) : null}
        {notice ? (
          <div className="inline-alert">
            <Icon name="info" />
            <span>{notice}</span>
          </div>
        ) : null}
        {!isTauriRuntime() ? (
          <div className="runtime-banner">
            <Icon name="info" />
            <span>浏览器预览：当前展示模拟数据，不会读取本机包管理器、项目目录或 SQLite 快照。</span>
          </div>
        ) : null}
        {page === "overview" ? (
          <OverviewPage
            data={data}
            isLoading={isLoading}
            scanProgress={scanProgress}
            comparison={history.comparison}
            onRefresh={refreshNow}
            onCancel={cancelScanNow}
            onNavigate={navigate}
            onStartUpgradePlan={startUpgradePlan}
          />
        ) : null}
        {diagnosticPageSet.has(page) ? (
          <nav className="diagnostic-nav" aria-label="诊断视图">
            {diagnosticPages.map((item) => (
              <button
                key={item}
                className={page === item ? "diagnostic-nav__item diagnostic-nav__item--active" : "diagnostic-nav__item"}
                onClick={() => navigate(item)}
              >
                {diagnosticLabels[item]}
              </button>
            ))}
          </nav>
        ) : null}
        {page === "packages" ? (
          <MemoPackagesPage
            packages={data.packages}
            scanSettings={data.scanSettings}
            onNavigate={navigate}
            onStartUpgradePlan={startUpgradePlan}
            onUpdateSettings={updateScanSettings}
            onRefresh={refreshNow}
          />
        ) : null}
        {page === "actions" ? (
          <ActionCenterContainer
            packages={data.packages}
            scanSettings={data.scanSettings}
            scannedAt={data.scannedAt}
            upgradePrefill={upgradePrefill}
            onApplyEnvironment={applyEnvironment}
            onUpdateSettings={updateScanSettings}
            onRefresh={refreshNow}
          />
        ) : null}
        {page === "projects" ? (
          <MemoProjectsPage
            projects={data.projects}
            workspaces={data.workspaces}
            scanRoots={data.scanRoots}
            ignoredDirectoryNames={data.scanSettings.defaultIgnoredDirectoryNames}
            onAddRoot={addRoot}
            onRemoveRoot={removeRoot}
            onRefresh={refreshNow}
          />
        ) : null}
        {page === "analysis" ? (
          <MemoProjectAnalysisPage
            view={analysisView}
            onChangeView={setAnalysisView}
            insights={data.dependencyInsights}
            projects={data.projects}
            workspaces={data.workspaces}
            scanRoots={data.scanRoots}
            ignoredDirectoryNames={data.scanSettings.defaultIgnoredDirectoryNames}
            onLoadGraph={getProjectDependencyGraph}
            onLoadReport={getProjectSupplyChainReport}
            onExportSbom={exportProjectSbom}
          />
        ) : null}
        {page === "runtimes" ? (
          <MemoRuntimesPage installations={data.runtimeInstallations} assessments={data.runtimeAssessments} />
        ) : null}
        {page === "history" ? (
          <MemoHistoryPage
            summaries={history.summaries}
            comparison={history.comparison}
            isLoading={history.isLoading}
            error={history.error}
            onCompare={history.compare}
            onExport={history.exportComparison}
          />
        ) : null}
        {page === "environment" ? <MemoEnvironmentPage data={data} onNavigate={navigate} /> : null}
        {page === "logs" ? <MemoLogsPage logs={data.logs} /> : null}
        {page === "settings" ? (
          <MemoSettingsPage
            scanSettings={data.scanSettings}
            onUpdateSettings={updateScanSettings}
            onExportReport={exportEnvironmentReport}
          />
        ) : null}
      </>
    );
  }

  return (
    <AppShell page={page} onNavigate={navigate}>
      {content}
    </AppShell>
  );
}
