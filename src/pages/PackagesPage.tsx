import { useDeferredValue, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import { UpdateBadge } from "../components/Status";
import { filterPackages, managerLabel } from "../lib/format";
import type { ManagedPackage, PackageManagerId, PageId, UpdateStatus } from "../types";

export function PackagesPage({ packages, onNavigate }: { packages: ManagedPackage[]; onNavigate: (page: PageId) => void }) {
  const [query, setQuery] = useState("");
  const [manager, setManager] = useState<"all" | PackageManagerId>("all");
  const [status, setStatus] = useState<"all" | UpdateStatus>("all");
  const deferredQuery = useDeferredValue(query);
  const filtered = filterPackages(packages, { query: deferredQuery, manager, status });

  return (
    <>
      <PageHeader title="软件包" description="统一查看系统软件、全局工具与更新状态。" actions={<button className="button button--secondary" onClick={() => onNavigate("actions")}>管理操作</button>} />
      <section className="toolbar" aria-label="软件包筛选">
        <label className="search-field"><Icon name="search" /><span className="sr-only">搜索软件包</span><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索软件包" /></label>
        <label className="select-field"><span>管理器</span><select value={manager} onChange={(event) => setManager(event.target.value as typeof manager)}><option value="all">全部</option>{Object.entries(managerLabel).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
        <label className="select-field"><span>状态</span><select value={status} onChange={(event) => setStatus(event.target.value as typeof status)}><option value="all">全部</option><option value="available">可更新</option><option value="upToDate">已是最新</option><option value="unknown">未知</option></select></label>
        <span className="toolbar__count">{filtered.length} 个结果</span>
      </section>
      <section className="panel package-panel">
        {filtered.length ? (
          <div className="table-wrap table-wrap--full"><table><thead><tr><th>软件包</th><th>来源</th><th>作用域</th><th>已安装</th><th>最新版本</th><th>状态</th></tr></thead><tbody>{filtered.map((pkg) => <tr key={pkg.id}><td><strong>{pkg.name}</strong><small className="table-subtitle">{pkg.id}</small></td><td><span className={`manager-chip manager-chip--${pkg.managerId}`}>{managerLabel[pkg.managerId]}</span></td><td>{pkg.scope === "system" ? "系统" : pkg.scope === "global" ? "全局" : "工具"}</td><td className="mono">{pkg.version}</td><td className="mono">{pkg.latestVersion ?? "—"}</td><td><UpdateBadge status={pkg.updateStatus} /></td></tr>)}</tbody></table></div>
        ) : <EmptyState title="没有匹配的软件包" description="调整搜索词或筛选条件后重试。" />}
      </section>
    </>
  );
}
