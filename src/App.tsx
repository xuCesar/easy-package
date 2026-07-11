import { useState } from "react";
import { AppShell } from "./components/AppShell";
import { isTauriRuntime } from "./api";
import { Icon } from "./components/Icon";
import { useDevPkg } from "./hooks/useDevPkg";
import { useSnapshotHistory } from "./hooks/useSnapshotHistory";
import { EnvironmentPage } from "./pages/EnvironmentPage";
import { DependenciesPage } from "./pages/DependenciesPage";
import { OverviewPage } from "./pages/OverviewPage";
import { PackagesPage } from "./pages/PackagesPage";
import { ProjectsPage } from "./pages/ProjectsPage";
import { RuntimesPage } from "./pages/RuntimesPage";
import { HistoryPage } from "./pages/HistoryPage";
import { SupplyChainPage } from "./pages/SupplyChainPage";
import { ActionCenterPage } from "./pages/ActionCenterPage";
import { usePackageActions } from "./hooks/usePackageActions";
import type { PageId } from "./types";

export function App() {
  const [page, setPage] = useState<PageId>("overview");
  const { data, isLoading, error, scanProgress, notice, refresh, cancelScan, addRoot, removeRoot, updateScanSettings, exportEnvironmentReport, getProjectDependencyGraph, getProjectSupplyChainReport, exportProjectSbom, applyEnvironment } = useDevPkg();
  const packageActions = usePackageActions(applyEnvironment, data?.scannedAt);
  const history = useSnapshotHistory(data?.scannedAt);

  let content;
  if (!data && isLoading) {
    content = <div className="app-state"><span className="scan-indicator"><Icon name="refresh" /></span><h1>正在扫描本机环境</h1><p>{scanProgress ? `扫描进度 ${scanProgress.completed}/${scanProgress.total}` : "读取包管理器版本、软件包和缓存信息…"}</p><button className="button button--secondary" onClick={() => void cancelScan()}>取消扫描</button></div>;
  } else if (!data) {
    content = <div className="app-state app-state--error"><Icon name="warning" /><h1>无法完成环境扫描</h1><p>{error}</p><button className="button button--primary" onClick={() => void refresh()}>重新扫描</button></div>;
  } else {
    content = (
      <>
        {error ? <div className="inline-alert inline-alert--error"><Icon name="warning" /><span>{error}</span><button onClick={() => void refresh()}>重试</button></div> : null}
        {notice ? <div className="inline-alert"><Icon name="info" /><span>{notice}</span></div> : null}
        {!isTauriRuntime() ? <div className="runtime-banner"><Icon name="info" /><span>浏览器预览：当前展示模拟数据，不会读取本机包管理器、项目目录或 SQLite 快照。</span></div> : null}
        {page === "overview" ? <OverviewPage data={data} isLoading={isLoading} scanProgress={scanProgress} comparison={history.comparison} onRefresh={() => void refresh()} onCancel={() => void cancelScan()} onNavigate={setPage} /> : null}
        {page === "packages" ? <PackagesPage packages={data.packages} /> : null}
        {page === "actions" ? <ActionCenterPage packages={data.packages} capabilities={packageActions.capabilities} plan={packageActions.plan} result={packageActions.result} audit={packageActions.audit} progress={packageActions.progress} isPlanning={packageActions.isPlanning} isExecuting={packageActions.isExecuting} isReconciling={packageActions.isReconciling} error={packageActions.error} onCreatePlan={packageActions.createPlan} onExecute={packageActions.executePlan} onCancel={packageActions.cancelAction} onReconcile={packageActions.reconcileAction} onClearPlan={packageActions.clearPlan} /> : null}
        {page === "projects" ? <ProjectsPage projects={data.projects} workspaces={data.workspaces} scanRoots={data.scanRoots} scanSettings={data.scanSettings} onAddRoot={addRoot} onRemoveRoot={removeRoot} onUpdateSettings={updateScanSettings} onExportReport={exportEnvironmentReport} onRefresh={() => void refresh()} /> : null}
        {page === "dependencies" ? <DependenciesPage insights={data.dependencyInsights} projects={data.projects} onLoadGraph={getProjectDependencyGraph} onExportSbom={exportProjectSbom} /> : null}
        {page === "supplyChain" ? <SupplyChainPage projects={data.projects} onLoadReport={getProjectSupplyChainReport} onExportSbom={exportProjectSbom} /> : null}
        {page === "runtimes" ? <RuntimesPage installations={data.runtimeInstallations} assessments={data.runtimeAssessments} /> : null}
        {page === "history" ? <HistoryPage summaries={history.summaries} comparison={history.comparison} isLoading={history.isLoading} error={history.error} onCompare={history.compare} onExport={history.exportComparison} /> : null}
        {page === "environment" ? <EnvironmentPage data={data} onNavigate={setPage} /> : null}
      </>
    );
  }

  return <AppShell page={page} onNavigate={setPage}>{content}</AppShell>;
}
