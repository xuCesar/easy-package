import { useMemo, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import type { DependencyInsight } from "../types";

interface DependenciesPageProps {
  insights: DependencyInsight[];
}

export function DependenciesPage({ insights }: DependenciesPageProps) {
  const [query, setQuery] = useState("");
  const [ecosystem, setEcosystem] = useState("all");
  const [onlyDivergent, setOnlyDivergent] = useState(false);
  const ecosystems = useMemo(() => [...new Set(insights.map((insight) => insight.ecosystem))].sort(), [insights]);
  const filtered = useMemo(() => insights.filter((insight) => {
    const matchesQuery = insight.name.toLowerCase().includes(query.trim().toLowerCase());
    return matchesQuery && (ecosystem === "all" || insight.ecosystem === ecosystem) && (!onlyDivergent || insight.hasVersionDivergence);
  }), [ecosystem, insights, onlyDivergent, query]);
  const divergentCount = insights.filter((insight) => insight.hasVersionDivergence).length;
  const totalProjects = new Set(insights.flatMap((insight) => insight.projects.map((project) => project.projectPath))).size;

  return (
    <>
      <PageHeader title="依赖" description="基于已选项目的直接声明依赖生成，不读取完整依赖树或修改本机环境。" />
      <section className="metrics dependency-metrics" aria-label="依赖摘要">
        <div className="metric"><span className="metric__icon"><Icon name="dependencies" /></span><div><strong>{insights.length}</strong><span>直接依赖</span></div></div>
        <div className="metric"><span className="metric__icon"><Icon name="projects" /></span><div><strong>{totalProjects}</strong><span>已索引项目</span></div></div>
        <div className="metric"><span className="metric__icon"><Icon name="warning" /></span><div><strong>{divergentCount}</strong><span>版本分歧</span></div></div>
      </section>
      <div className="toolbar" role="search">
        <label className="search-field"><Icon name="search" /><span className="sr-only">搜索依赖</span><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索依赖名称" /></label>
        <label className="select-field">生态<select aria-label="依赖生态" value={ecosystem} onChange={(event) => setEcosystem(event.target.value)}><option value="all">全部生态</option>{ecosystems.map((item) => <option key={item} value={item}>{item}</option>)}</select></label>
        <label className="checkbox-field"><input type="checkbox" checked={onlyDivergent} onChange={(event) => setOnlyDivergent(event.target.checked)} />仅看版本分歧</label>
        <span className="toolbar__count">{filtered.length} 个结果</span>
      </div>
      <section className="panel dependency-panel">
        {filtered.length ? <div className="dependency-list">{filtered.map((insight) => (
          <article className="dependency-item" key={`${insight.ecosystem}:${insight.name}`}>
            <div className="dependency-item__summary"><div><span className="manager-chip">{insight.ecosystem}</span><h2>{insight.name}</h2><p>{insight.projectCount} 个项目 · {insight.versionRequirements.join("、")}</p></div>{insight.hasVersionDivergence ? <span className="divergence-badge"><Icon name="warning" />版本分歧</span> : null}</div>
            <div className="dependency-projects">{insight.projects.map((project) => <div key={project.projectPath}><strong>{project.projectName}</strong><code title={project.projectPath}>{project.projectPath}</code><span className="mono">{project.versionRequirement}</span><span>{project.scopes.join(" · ")}</span></div>)}</div>
          </article>
        ))}</div> : <EmptyState icon="dependencies" title="没有匹配的依赖" description="调整搜索条件，或先在“项目”中添加需要索引的目录。" />}
      </section>
    </>
  );
}
