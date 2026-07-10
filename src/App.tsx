import { useState } from "react";
import { AppShell } from "./components/AppShell";
import { Icon } from "./components/Icon";
import { useDevPkg } from "./hooks/useDevPkg";
import { EnvironmentPage } from "./pages/EnvironmentPage";
import { OverviewPage } from "./pages/OverviewPage";
import { PackagesPage } from "./pages/PackagesPage";
import { ProjectsPage } from "./pages/ProjectsPage";
import type { PageId } from "./types";

export function App() {
  const [page, setPage] = useState<PageId>("overview");
  const { data, isLoading, error, refresh, addRoot, removeRoot } = useDevPkg();

  let content;
  if (!data && isLoading) {
    content = <div className="app-state"><span className="scan-indicator"><Icon name="refresh" /></span><h1>正在扫描本机环境</h1><p>读取包管理器版本、软件包和缓存信息…</p></div>;
  } else if (!data) {
    content = <div className="app-state app-state--error"><Icon name="warning" /><h1>无法完成环境扫描</h1><p>{error}</p><button className="button button--primary" onClick={() => void refresh()}>重新扫描</button></div>;
  } else {
    content = (
      <>
        {error ? <div className="inline-alert inline-alert--error"><Icon name="warning" /><span>{error}</span><button onClick={() => void refresh()}>重试</button></div> : null}
        {page === "overview" ? <OverviewPage data={data} isLoading={isLoading} onRefresh={() => void refresh()} onNavigate={setPage} /> : null}
        {page === "packages" ? <PackagesPage packages={data.packages} /> : null}
        {page === "projects" ? <ProjectsPage projects={data.projects} scanRoots={data.scanRoots} onAddRoot={addRoot} onRemoveRoot={removeRoot} onRefresh={() => void refresh()} /> : null}
        {page === "environment" ? <EnvironmentPage data={data} /> : null}
      </>
    );
  }

  return <AppShell page={page} onNavigate={setPage}>{content}</AppShell>;
}
