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
import { ProjectsPage } from "./pages/ProjectsPage";
import { RuntimesPage } from "./pages/RuntimesPage";
import { SettingsPage } from "./pages/SettingsPage";
import type { PageId } from "./types";

const localWorkspacePages = [
  { id: "environment", label: "环境" },
  { id: "runtimes", label: "运行时" },
  { id: "history", label: "历史" },
  { id: "logs", label: "日志" },
] as const satisfies ReadonlyArray<{ id: PageId; label: string }>;
const projectWorkspacePages = [{ id: "projects", label: "项目列表" }] as const satisfies ReadonlyArray<{
  id: PageId;
  label: string;
}>;
const localWorkspacePageSet = new Set<PageId>(localWorkspacePages.map((item) => item.id));
const projectWorkspacePageSet = new Set<PageId>(projectWorkspacePages.map((item) => item.id));

function WorkspaceNavigation({
  label,
  pages,
  currentPage,
  onNavigate,
}: {
  label: string;
  pages: ReadonlyArray<{ id: PageId; label: string }>;
  currentPage: PageId;
  onNavigate: (page: PageId) => void;
}) {
  return (
    <nav className="workspace-nav" aria-label={label}>
      {pages.map((item) => (
        <button
          key={item.id}
          className={
            currentPage === item.id ? "workspace-nav__item workspace-nav__item--active" : "workspace-nav__item"
          }
          onClick={() => onNavigate(item.id)}
          aria-current={currentPage === item.id ? "page" : undefined}
        >
          {item.label}
        </button>
      ))}
    </nav>
  );
}

// 扫描进度高频更新只应重渲染消费它的概览页；其余页面通过 memo + 稳定 props 跳过。
const MemoPackagesPage = memo(PackagesPage);
const MemoProjectsPage = memo(ProjectsPage);
const MemoRuntimesPage = memo(RuntimesPage);
const MemoHistoryPage = memo(HistoryPage);
const MemoEnvironmentPage = memo(EnvironmentPage);
const MemoLogsPage = memo(LogsPage);
const MemoSettingsPage = memo(SettingsPage);

export function App() {
  const [page, setPage] = useState<PageId>("overview");
  const [upgradePrefill, setUpgradePrefill] = useState<UpgradePlanPrefill>();
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
        {localWorkspacePageSet.has(page) ? (
          <WorkspaceNavigation
            label="本机工作区导航"
            pages={localWorkspacePages}
            currentPage={page}
            onNavigate={navigate}
          />
        ) : null}
        {projectWorkspacePageSet.has(page) ? (
          <WorkspaceNavigation
            label="项目工作区导航"
            pages={data.scanRoots.length > 0 ? projectWorkspacePages : projectWorkspacePages.slice(0, 1)}
            currentPage={page}
            onNavigate={navigate}
          />
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
            dependencyInsights={data.dependencyInsights}
            runtimeAssessments={data.runtimeAssessments}
            ignoredDirectoryNames={data.scanSettings.defaultIgnoredDirectoryNames}
            onAddRoot={addRoot}
            onRemoveRoot={removeRoot}
            onRefresh={refreshNow}
            onLoadGraph={getProjectDependencyGraph}
            onLoadReport={getProjectSupplyChainReport}
            onExportSbom={exportProjectSbom}
          />
        ) : null}
        {page === "runtimes" ? <MemoRuntimesPage installations={data.runtimeInstallations} /> : null}
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
