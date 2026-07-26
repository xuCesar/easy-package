import { type ReactNode, useState } from "react";
import { isTauriRuntime } from "./api";
import { AppShell } from "./components/AppShell";
import { Icon } from "./components/Icon";
import { useCatalogSearch } from "./hooks/useCatalogSearch";
import { useDevPkg } from "./hooks/useDevPkg";
import { usePackageActions } from "./hooks/usePackageActions";
import { useSnapshotHistory } from "./hooks/useSnapshotHistory";
import type { UpgradePlanPrefill } from "./lib/upgradePlanBridge";
import { ActionCenterPage } from "./pages/ActionCenterPage";
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

export function App() {
  const [page, setPage] = useState<PageId>("overview");
  const [upgradePrefill, setUpgradePrefill] = useState<UpgradePlanPrefill>();
  const [analysisView, setAnalysisView] = useState<ProjectAnalysisView>("index");
  const navigate = (next: PageId) => {
    setUpgradePrefill(undefined);
    setPage(next);
  };
  const openAnalysis = (view: ProjectAnalysisView) => {
    setAnalysisView(view);
    navigate("analysis");
  };
  const startUpgradePlan = (prefill: UpgradePlanPrefill) => {
    setUpgradePrefill(prefill);
    setPage("actions");
  };
  const {
    data,
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
  const packageActions = usePackageActions(applyEnvironment, data?.scannedAt);
  const catalogSearch = useCatalogSearch();
  const history = useSnapshotHistory(data?.scannedAt);

  let content: ReactNode;
  if (!data && isLoading) {
    content = (
      <div className="app-state">
        <span className="scan-indicator">
          <Icon name="refresh" />
        </span>
        <h1>正在扫描本机环境</h1>
        <p>
          {scanProgress
            ? `扫描进度 ${scanProgress.completed}/${scanProgress.total}`
            : "读取包管理器版本、软件包和缓存信息…"}
        </p>
        <button className="button button--secondary" onClick={() => void cancelScan()}>
          取消扫描
        </button>
      </div>
    );
  } else if (!data) {
    content = (
      <div className="app-state app-state--error">
        <Icon name="warning" />
        <h1>无法完成环境扫描</h1>
        <p>{error}</p>
        <button className="button button--primary" onClick={() => void refresh()}>
          重新扫描
        </button>
      </div>
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
            onRefresh={() => void refresh()}
            onCancel={() => void cancelScan()}
            onNavigate={navigate}
            onOpenAnalysis={openAnalysis}
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
          <PackagesPage packages={data.packages} onNavigate={navigate} onStartUpgradePlan={startUpgradePlan} />
        ) : null}
        {page === "actions" ? (
          <ActionCenterPage
            upgradePrefill={upgradePrefill}
            packages={data.packages}
            scanSettings={data.scanSettings}
            capabilities={packageActions.capabilities}
            catalogResponse={catalogSearch.response}
            isCatalogSearching={catalogSearch.isSearching}
            catalogError={catalogSearch.error}
            plan={packageActions.plan}
            result={packageActions.result}
            audit={packageActions.audit}
            progress={packageActions.progress}
            isPlanning={packageActions.isPlanning}
            isExecuting={packageActions.isExecuting}
            isReconciling={packageActions.isReconciling}
            error={packageActions.error}
            onSearchCatalog={catalogSearch.search}
            onCancelCatalogSearch={catalogSearch.cancel}
            onClearCatalogSearch={catalogSearch.clear}
            onCreatePlan={packageActions.createPlan}
            onExecute={packageActions.executePlan}
            onCancel={packageActions.cancelAction}
            onReconcile={packageActions.reconcileAction}
            onClearPlan={packageActions.clearPlan}
          />
        ) : null}
        {page === "projects" ? (
          <ProjectsPage
            projects={data.projects}
            workspaces={data.workspaces}
            scanRoots={data.scanRoots}
            ignoredDirectoryNames={data.scanSettings.defaultIgnoredDirectoryNames}
            onAddRoot={addRoot}
            onRemoveRoot={removeRoot}
            onRefresh={() => void refresh()}
          />
        ) : null}
        {page === "analysis" ? (
          <ProjectAnalysisPage
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
          <RuntimesPage installations={data.runtimeInstallations} assessments={data.runtimeAssessments} />
        ) : null}
        {page === "history" ? (
          <HistoryPage
            summaries={history.summaries}
            comparison={history.comparison}
            isLoading={history.isLoading}
            error={history.error}
            onCompare={history.compare}
            onExport={history.exportComparison}
          />
        ) : null}
        {page === "environment" ? <EnvironmentPage data={data} onNavigate={navigate} /> : null}
        {page === "logs" ? <LogsPage logs={data.logs} /> : null}
        {page === "settings" ? (
          <SettingsPage
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
