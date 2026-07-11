import { useMemo, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import type { DependencyGraphNode, DependencyInsight, ProjectDependencyGraph, ProjectMetadata, ReportExportResult } from "../types";

interface DependenciesPageProps {
  insights: DependencyInsight[];
  projects: ProjectMetadata[];
  onLoadGraph: (projectPath: string) => Promise<ProjectDependencyGraph>;
  onExportSbom: (projectPath: string) => Promise<ReportExportResult>;
}

const completenessLabel = {
  complete: "完整",
  partial: "部分",
  unsupported: "不支持",
  invalid: "无效",
};

export function DependenciesPage({ insights, projects, onLoadGraph, onExportSbom }: DependenciesPageProps) {
  const [view, setView] = useState<"insights" | "graph">("insights");
  const [query, setQuery] = useState("");
  const [ecosystem, setEcosystem] = useState("all");
  const [onlyDivergent, setOnlyDivergent] = useState(false);
  const [onlyRisky, setOnlyRisky] = useState(false);
  const [onlyResolutionRisk, setOnlyResolutionRisk] = useState(false);
  const [selectedProjectPath, setSelectedProjectPath] = useState(projects.find((project) => project.dependencyGraphSummary)?.path ?? projects[0]?.path ?? "");
  const [graph, setGraph] = useState<ProjectDependencyGraph>();
  const [graphQuery, setGraphQuery] = useState("");
  const [graphScope, setGraphScope] = useState<"all" | "direct" | "transitive" | "duplicates">("all");
  const [selectedNodeId, setSelectedNodeId] = useState<string>();
  const [isLoadingGraph, setIsLoadingGraph] = useState(false);
  const [isExporting, setIsExporting] = useState(false);
  const [actionError, setActionError] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const ecosystems = useMemo(() => [...new Set(insights.map((insight) => insight.ecosystem))].sort(), [insights]);
  const filtered = useMemo(() => insights.filter((insight) => {
    const matchesQuery = insight.name.toLowerCase().includes(query.trim().toLowerCase());
    return matchesQuery && (ecosystem === "all" || insight.ecosystem === ecosystem) && (!onlyDivergent || insight.hasVersionDivergence) && (!onlyRisky || insight.hasHealthRisk) && (!onlyResolutionRisk || insight.hasResolutionRisk);
  }), [ecosystem, insights, onlyDivergent, onlyResolutionRisk, onlyRisky, query]);
  const divergentCount = insights.filter((insight) => insight.hasVersionDivergence).length;
  const totalProjects = new Set(insights.flatMap((insight) => insight.projects.map((project) => project.projectPath))).size;
  const duplicateNames = useMemo(() => {
    const versions = new Map<string, Set<string>>();
    graph?.nodes.filter((node) => node.kind === "package").forEach((node) => {
      const key = `${node.ecosystem}:${node.name}`;
      const values = versions.get(key) ?? new Set<string>();
      values.add(node.version);
      versions.set(key, values);
    });
    return new Set([...versions.entries()].filter(([, values]) => values.size > 1).map(([key]) => key));
  }, [graph]);
  const graphNodes = useMemo(() => graph?.nodes.filter((node) => {
    if (node.kind === "project") return false;
    if (!node.name.toLowerCase().includes(graphQuery.trim().toLowerCase())) return false;
    if (graphScope === "direct" && !node.direct) return false;
    if (graphScope === "transitive" && node.direct) return false;
    if (graphScope === "duplicates" && !duplicateNames.has(`${node.ecosystem}:${node.name}`)) return false;
    return true;
  }) ?? [], [duplicateNames, graph, graphQuery, graphScope]);
  const selectedNode = graph?.nodes.find((node) => node.id === selectedNodeId);
  const selectedPath = graph && selectedNode ? shortestDependencyPath(graph, selectedNode.id) : [];
  const outgoing = graph && selectedNode ? relatedNodes(graph, selectedNode.id, "outgoing") : [];
  const incoming = graph && selectedNode ? relatedNodes(graph, selectedNode.id, "incoming") : [];

  const loadGraph = async () => {
    if (!selectedProjectPath) return;
    setIsLoadingGraph(true);
    setActionError(undefined);
    setNotice(undefined);
    setSelectedNodeId(undefined);
    try {
      setGraph(await onLoadGraph(selectedProjectPath));
    } catch (error) {
      setGraph(undefined);
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsLoadingGraph(false);
    }
  };

  const exportSbom = async () => {
    if (!graph) return;
    setIsExporting(true);
    setActionError(undefined);
    setNotice(undefined);
    try {
      const result = await onExportSbom(graph.projectPath);
      setNotice(result.saved ? "CycloneDX SBOM 已导出。" : "已取消导出。");
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <>
      <PageHeader title="依赖" description="跨项目汇总直接声明依赖，并按需从 npm、pnpm 与 Cargo 锁文件生成完整只读依赖图。" />
      {actionError ? <div className="inline-alert inline-alert--error"><Icon name="warning" /><span>{actionError}</span></div> : null}
      {notice ? <div className="inline-alert"><Icon name="info" /><span>{notice}</span></div> : null}
      <div className="view-tabs" role="tablist" aria-label="依赖视图">
        <button role="tab" aria-selected={view === "insights"} className={view === "insights" ? "view-tab view-tab--active" : "view-tab"} onClick={() => setView("insights")}>跨项目汇总</button>
        <button role="tab" aria-selected={view === "graph"} className={view === "graph" ? "view-tab view-tab--active" : "view-tab"} onClick={() => setView("graph")}>项目依赖图</button>
      </div>
      {view === "insights" ? <>
        <section className="metrics dependency-metrics" aria-label="依赖摘要">
          <div className="metric"><span className="metric__icon"><Icon name="dependencies" /></span><div><strong>{insights.length}</strong><span>直接依赖</span></div></div>
          <div className="metric"><span className="metric__icon"><Icon name="projects" /></span><div><strong>{totalProjects}</strong><span>已索引项目</span></div></div>
          <div className="metric"><span className="metric__icon"><Icon name="warning" /></span><div><strong>{divergentCount}</strong><span>版本分歧</span></div></div>
        </section>
        <div className="toolbar" role="search">
          <label className="search-field"><Icon name="search" /><span className="sr-only">搜索依赖</span><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索依赖名称" /></label>
          <label className="select-field">生态<select aria-label="依赖生态" value={ecosystem} onChange={(event) => setEcosystem(event.target.value)}><option value="all">全部生态</option>{ecosystems.map((item) => <option key={item} value={item}>{item}</option>)}</select></label>
          <label className="checkbox-field"><input type="checkbox" checked={onlyDivergent} onChange={(event) => setOnlyDivergent(event.target.checked)} />仅看版本分歧</label>
          <label className="checkbox-field"><input type="checkbox" checked={onlyRisky} onChange={(event) => setOnlyRisky(event.target.checked)} />仅看健康风险</label>
          <label className="checkbox-field"><input type="checkbox" checked={onlyResolutionRisk} onChange={(event) => setOnlyResolutionRisk(event.target.checked)} />仅看解析异常</label>
          <span className="toolbar__count">{filtered.length} 个结果</span>
        </div>
        <section className="panel dependency-panel">
          {filtered.length ? <div className="dependency-list">{filtered.map((insight) => (
            <article className="dependency-item" key={`${insight.ecosystem}:${insight.name}`}>
              <div className="dependency-item__summary"><div><span className="manager-chip">{insight.ecosystem}</span><h2>{insight.name}</h2><p>声明：{insight.versionRequirements.join("、")} · 已解析：{insight.resolvedVersions.join("、") || "未解析"}</p></div>{insight.hasVersionDivergence || insight.hasResolvedVersionDivergence || insight.hasResolutionRisk ? <span className="divergence-badge"><Icon name="warning" />{insight.hasResolutionRisk ? "解析异常" : "版本分歧"}</span> : null}</div>
              <div className="dependency-projects">{insight.projects.map((project) => <div key={project.projectPath}><strong>{project.projectName}</strong><code title={project.projectPath}>{project.projectPath}</code><span className="mono">{project.versionRequirement} → {project.resolvedVersion ?? "未解析"}</span><span>{project.resolutionSource ?? project.scopes.join(" · ")}</span></div>)}</div>
            </article>
          ))}</div> : <EmptyState icon="dependencies" title="没有匹配的依赖" description="调整搜索条件，或先在“项目”中添加需要索引的目录。" />}
        </section>
      </> : <>
        <section className="panel graph-controls" aria-label="项目依赖图设置">
          <div className="graph-controls__body">
            <label className="select-field">项目<select aria-label="依赖图项目" value={selectedProjectPath} onChange={(event) => { setSelectedProjectPath(event.target.value); setGraph(undefined); setSelectedNodeId(undefined); }}><option value="">选择项目</option>{projects.map((project) => <option key={project.path} value={project.path}>{project.name}{project.dependencyGraphSummary ? ` · ${completenessLabel[project.dependencyGraphSummary.completeness]}` : ""}</option>)}</select></label>
            <button className="button button--primary" onClick={() => void loadGraph()} disabled={!selectedProjectPath || isLoadingGraph}>{isLoadingGraph ? "解析中…" : "解析依赖图"}</button>
            <button className="button button--secondary" onClick={() => void exportSbom()} disabled={!graph || graph.completeness === "unsupported" || graph.completeness === "invalid" || isExporting}>{isExporting ? "导出中…" : "导出 CycloneDX SBOM"}</button>
          </div>
          <p>完整图按需读取当前锁文件，不写入 SQLite 快照；快照只保留图摘要和锁文件摘要。</p>
        </section>
        {graph ? <>
          <section className="metrics graph-metrics" aria-label="依赖图摘要">
            <div className="metric"><div><strong>{graph.summary.nodeCount - 1}</strong><span>依赖节点</span></div></div>
            <div className="metric"><div><strong>{graph.summary.edgeCount}</strong><span>依赖关系</span></div></div>
            <div className="metric"><div><strong>{graph.summary.directCount}</strong><span>直接依赖</span></div></div>
            <div className="metric"><div><strong>{graph.summary.transitiveCount}</strong><span>传递依赖</span></div></div>
          </section>
          <div className={`graph-completeness graph-completeness--${graph.completeness}`}><strong>{completenessLabel[graph.completeness]}</strong><span>{graph.sources.join("、") || "无受支持锁文件"} · 摘要 {graph.sourceDigest || "—"}</span></div>
          {graph.warnings.map((warning) => <div className="inline-alert" key={warning}><Icon name="warning" /><span>{warning}</span></div>)}
          <div className="toolbar" role="search">
            <label className="search-field"><Icon name="search" /><span className="sr-only">搜索图节点</span><input value={graphQuery} onChange={(event) => { setGraphQuery(event.target.value); setSelectedNodeId(undefined); }} placeholder="搜索图节点" /></label>
            <label className="select-field">范围<select aria-label="依赖图范围" value={graphScope} onChange={(event) => { setGraphScope(event.target.value as typeof graphScope); setSelectedNodeId(undefined); }}><option value="all">全部</option><option value="direct">直接依赖</option><option value="transitive">传递依赖</option><option value="duplicates">重复版本</option></select></label>
            <span className="toolbar__count">{graphNodes.length} 个节点</span>
          </div>
          <div className="graph-layout">
            <section className="panel graph-node-panel">
              <div className="panel__header"><h2>依赖节点</h2><span className="count-label">{graphNodes.length}</span></div>
              {graphNodes.length ? <div className="graph-node-list">{graphNodes.map((node) => <button className={selectedNodeId === node.id ? "graph-node graph-node--active" : "graph-node"} key={node.id} onClick={() => setSelectedNodeId(node.id)}><span className="manager-chip">{node.ecosystem}</span><strong>{node.name}</strong><code>{node.version}</code>{node.direct ? <small>直接</small> : <small>传递</small>}</button>)}</div> : <EmptyState icon="dependencies" title="没有匹配的图节点" description="调整名称或依赖范围筛选。" />}
            </section>
            <section className="panel graph-detail-panel">
              <div className="panel__header"><h2>依赖路径</h2></div>
              {selectedNode ? <div className="graph-detail"><div className="graph-detail__title"><span className="manager-chip">{selectedNode.ecosystem}</span><strong>{selectedNode.name}</strong><code>{selectedNode.version}</code></div><div><h3>从项目根到该依赖</h3><p className="dependency-path">{selectedPath.map((node) => node.name).join(" → ") || "没有可达路径"}</p></div><div className="graph-relations"><section><h3>依赖它的节点</h3>{incoming.length ? incoming.map((node) => <span key={node.id}>{node.name} <code>{node.version}</code></span>) : <p>没有上游节点</p>}</section><section><h3>它依赖的节点</h3>{outgoing.length ? outgoing.map((node) => <span key={node.id}>{node.name} <code>{node.version}</code></span>) : <p>没有下游节点</p>}</section></div></div> : <EmptyState icon="dependencies" title="选择一个依赖节点" description="查看从项目根到该依赖的最短路径，以及正向和反向关系。" />}
            </section>
          </div>
        </> : <section className="panel"><EmptyState icon="dependencies" title="尚未解析项目依赖图" description="选择项目并按需读取当前锁文件；不会执行包管理器命令。" /></section>}
      </>}
    </>
  );
}

function relatedNodes(graph: ProjectDependencyGraph, nodeId: string, direction: "incoming" | "outgoing"): DependencyGraphNode[] {
  const ids = new Set(graph.edges.filter((edge) => direction === "outgoing" ? edge.from === nodeId : edge.to === nodeId).map((edge) => direction === "outgoing" ? edge.to : edge.from));
  return graph.nodes.filter((node) => ids.has(node.id));
}

function shortestDependencyPath(graph: ProjectDependencyGraph, targetId: string): DependencyGraphNode[] {
  const root = graph.nodes.find((node) => node.kind === "project");
  if (!root) return [];
  const adjacency = new Map<string, string[]>();
  graph.edges.forEach((edge) => adjacency.set(edge.from, [...(adjacency.get(edge.from) ?? []), edge.to]));
  const queue: Array<{ id: string; path: string[] }> = [{ id: root.id, path: [root.id] }];
  const visited = new Set<string>();
  while (queue.length) {
    const current = queue.shift();
    if (!current || visited.has(current.id)) continue;
    if (current.id === targetId) return current.path.map((id) => graph.nodes.find((node) => node.id === id)).filter((node): node is DependencyGraphNode => Boolean(node));
    visited.add(current.id);
    (adjacency.get(current.id) ?? []).forEach((id) => queue.push({ id, path: [...current.path, id] }));
  }
  return [];
}
