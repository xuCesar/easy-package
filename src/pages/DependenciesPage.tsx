import { useMemo, useRef, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { apiErrorMessage } from "../lib/apiError";
import { buildProjectAnalysisOptions } from "../lib/projectAnalysisOptions";
import { isIgnoredScanPath } from "../lib/projectPaths";
import type {
  DependencyGraphNode,
  DependencyInsight,
  DependencyProjectUsage,
  ProjectDependencyGraph,
  ProjectMetadata,
  ProjectWorkspace,
  ReportExportResult,
} from "../types";

interface DependenciesPageProps {
  view: "index" | "graph";
  embedded?: boolean;
  insights: DependencyInsight[];
  projects: ProjectMetadata[];
  workspaces?: ProjectWorkspace[];
  scanRoots?: string[];
  ignoredDirectoryNames?: string[];
  projectPath: string;
  onSelectProject: (path: string) => void;
  showIgnoredProjects: boolean;
  onToggleIgnoredProjects: (show: boolean) => void;
  onLoadGraph: (projectPath: string) => Promise<ProjectDependencyGraph>;
  onExportSbom: (projectPath: string) => Promise<ReportExportResult>;
}

interface DependencyUsageDisplay extends DependencyProjectUsage {
  key: string;
  isIgnored: boolean;
  isWorkspace: boolean;
  memberCount: number;
}

const completenessLabel = {
  complete: "完整",
  partial: "部分",
  unsupported: "不支持",
  invalid: "无效",
};

export function DependenciesPage({
  view,
  embedded = false,
  insights,
  projects,
  workspaces = [],
  scanRoots = [],
  ignoredDirectoryNames = [],
  projectPath: selectedProjectPath,
  onSelectProject,
  showIgnoredProjects,
  onToggleIgnoredProjects,
  onLoadGraph,
  onExportSbom,
}: DependenciesPageProps) {
  const [query, setQuery] = useState("");
  const [ecosystem, setEcosystem] = useState("all");
  const [onlyDivergent, setOnlyDivergent] = useState(false);
  const [onlyRisky, setOnlyRisky] = useState(false);
  const [onlyResolutionRisk, setOnlyResolutionRisk] = useState(false);
  const projectOptions = useMemo(
    () => buildProjectAnalysisOptions(projects, workspaces, scanRoots, ignoredDirectoryNames, showIgnoredProjects),
    [ignoredDirectoryNames, projects, scanRoots, showIgnoredProjects, workspaces],
  );
  const [graph, setGraph] = useState<ProjectDependencyGraph>();
  const [graphQuery, setGraphQuery] = useState("");
  const [graphScope, setGraphScope] = useState<"all" | "direct" | "transitive" | "duplicates">("all");
  const [selectedNodeId, setSelectedNodeId] = useState<string>();
  const [isLoadingGraph, setIsLoadingGraph] = useState(false);
  const [isExporting, setIsExporting] = useState(false);
  const [actionError, setActionError] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const graphRequestToken = useRef(0);
  const ecosystems = useMemo(() => [...new Set(insights.map((insight) => insight.ecosystem))].sort(), [insights]);
  const insightDisplays = useMemo(
    () =>
      insights
        .map((insight) => ({
          insight,
          projects: buildDependencyUsageDisplays(
            insight.projects,
            workspaces,
            scanRoots,
            ignoredDirectoryNames,
            showIgnoredProjects,
          ),
        }))
        .filter((item) => item.projects.length > 0),
    [ignoredDirectoryNames, insights, scanRoots, showIgnoredProjects, workspaces],
  );
  const filtered = useMemo(
    () =>
      insightDisplays.filter(({ insight }) => {
        const matchesQuery = insight.name.toLowerCase().includes(query.trim().toLowerCase());
        return (
          matchesQuery &&
          (ecosystem === "all" || insight.ecosystem === ecosystem) &&
          (!onlyDivergent || insight.hasVersionDivergence) &&
          (!onlyRisky || insight.hasHealthRisk) &&
          (!onlyResolutionRisk || insight.hasResolutionRisk)
        );
      }),
    [ecosystem, insightDisplays, onlyDivergent, onlyResolutionRisk, onlyRisky, query],
  );
  const divergentCount = insightDisplays.filter(({ insight }) => insight.hasVersionDivergence).length;
  const totalProjects = new Set(insightDisplays.flatMap(({ projects: usages }) => usages.map((project) => project.key)))
    .size;
  const duplicateNames = useMemo(() => {
    const versions = new Map<string, Set<string>>();
    graph?.nodes
      .filter((node) => node.kind === "package")
      .forEach((node) => {
        const key = `${node.ecosystem}:${node.name}`;
        const values = versions.get(key) ?? new Set<string>();
        values.add(node.version);
        versions.set(key, values);
      });
    return new Set([...versions.entries()].filter(([, values]) => values.size > 1).map(([key]) => key));
  }, [graph]);
  const graphNodes = useMemo(
    () =>
      graph?.nodes.filter((node) => {
        if (node.kind === "project") return false;
        if (!node.name.toLowerCase().includes(graphQuery.trim().toLowerCase())) return false;
        if (graphScope === "direct" && !node.direct) return false;
        if (graphScope === "transitive" && node.direct) return false;
        if (graphScope === "duplicates" && !duplicateNames.has(`${node.ecosystem}:${node.name}`)) return false;
        return true;
      }) ?? [],
    [duplicateNames, graph, graphQuery, graphScope],
  );
  const selectedNode = graph?.nodes.find((node) => node.id === selectedNodeId);
  const selectedPath = graph && selectedNode ? shortestDependencyPath(graph, selectedNode.id) : [];
  const outgoing = graph && selectedNode ? relatedNodes(graph, selectedNode.id, "outgoing") : [];
  const incoming = graph && selectedNode ? relatedNodes(graph, selectedNode.id, "incoming") : [];

  const selectProject = (path: string) => {
    graphRequestToken.current += 1;
    onSelectProject(path);
    setGraph(undefined);
    setSelectedNodeId(undefined);
  };

  const loadGraph = async () => {
    if (!selectedProjectPath) return;
    graphRequestToken.current += 1;
    const token = graphRequestToken.current;
    setIsLoadingGraph(true);
    setActionError(undefined);
    setNotice(undefined);
    setSelectedNodeId(undefined);
    try {
      const graph = await onLoadGraph(selectedProjectPath);
      if (graphRequestToken.current !== token) return;
      setGraph(graph);
    } catch (error) {
      if (graphRequestToken.current !== token) return;
      setGraph(undefined);
      setActionError(apiErrorMessage(error, "依赖分析失败，请重试。"));
    } finally {
      if (graphRequestToken.current === token) {
        setIsLoadingGraph(false);
      }
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
      setActionError(apiErrorMessage(error, "依赖分析失败，请重试。"));
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <>
      {actionError ? (
        <div className="inline-alert inline-alert--error">
          <Icon name="warning" />
          <span>{actionError}</span>
        </div>
      ) : null}
      {notice ? (
        <div className="inline-alert">
          <Icon name="info" />
          <span>{notice}</span>
        </div>
      ) : null}
      {view === "index" ? (
        <>
          <section className="metrics dependency-metrics" aria-label="依赖摘要">
            <div className="metric">
              <span className="metric__icon">
                <Icon name="dependencies" />
              </span>
              <div>
                <strong>{insights.length}</strong>
                <span>直接依赖</span>
              </div>
            </div>
            <div className="metric">
              <span className="metric__icon">
                <Icon name="projects" />
              </span>
              <div>
                <strong>{totalProjects}</strong>
                <span>已索引项目</span>
              </div>
            </div>
            <div className="metric">
              <span className="metric__icon">
                <Icon name="warning" />
              </span>
              <div>
                <strong>{divergentCount}</strong>
                <span>版本分歧</span>
              </div>
            </div>
          </section>
          <div className="toolbar" role="search">
            <label className="search-field">
              <Icon name="search" />
              <span className="sr-only">搜索依赖</span>
              <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索依赖名称" />
            </label>
            <label className="select-field">
              生态
              <select aria-label="依赖生态" value={ecosystem} onChange={(event) => setEcosystem(event.target.value)}>
                <option value="all">全部生态</option>
                {ecosystems.map((item) => (
                  <option key={item} value={item}>
                    {item}
                  </option>
                ))}
              </select>
            </label>
            <label className="checkbox-field">
              <input
                type="checkbox"
                checked={onlyDivergent}
                onChange={(event) => setOnlyDivergent(event.target.checked)}
              />
              仅看版本分歧
            </label>
            <label className="checkbox-field">
              <input type="checkbox" checked={onlyRisky} onChange={(event) => setOnlyRisky(event.target.checked)} />
              仅看健康风险
            </label>
            <label className="checkbox-field">
              <input
                type="checkbox"
                checked={onlyResolutionRisk}
                onChange={(event) => setOnlyResolutionRisk(event.target.checked)}
              />
              仅看解析异常
            </label>
            <span className="toolbar__count">{filtered.length} 个结果</span>
          </div>
          <section className="panel dependency-panel">
            {filtered.length ? (
              <div className="dependency-list">
                {filtered.map(({ insight, projects: usageDisplays }) => (
                  <article className="dependency-item" key={`${insight.ecosystem}:${insight.name}`}>
                    <div className="dependency-item__summary">
                      <div>
                        <span className="manager-chip">{insight.ecosystem}</span>
                        <h2>{insight.name}</h2>
                        <p>
                          声明：{insight.versionRequirements.join("、")} · 已解析：
                          {insight.resolvedVersions.join("、") || "未解析"}
                        </p>
                      </div>
                      {insight.hasVersionDivergence ||
                      insight.hasResolvedVersionDivergence ||
                      insight.hasResolutionRisk ? (
                        <span className="divergence-badge">
                          <Icon name="warning" />
                          {insight.hasResolutionRisk ? "解析异常" : "版本分歧"}
                        </span>
                      ) : null}
                    </div>
                    <div className="dependency-projects">
                      {usageDisplays.map((project) => (
                        <div key={project.key}>
                          <strong>
                            {project.projectName}
                            {project.isWorkspace ? ` · ${project.memberCount} 个引用项目` : ""}
                            {project.isIgnored ? " · 已忽略" : ""}
                          </strong>
                          <code title={project.projectPath}>{project.projectPath}</code>
                          <span className="mono">
                            {project.versionRequirement} → {project.resolvedVersion ?? "未解析"}
                          </span>
                          <span>{project.resolutionSource ?? project.scopes.join(" · ")}</span>
                        </div>
                      ))}
                    </div>
                  </article>
                ))}
              </div>
            ) : (
              <EmptyState
                icon="dependencies"
                title="没有匹配的依赖"
                description="调整搜索条件，或先在“项目”中添加需要索引的目录。"
              />
            )}
          </section>
        </>
      ) : (
        <>
          <section className="panel graph-controls" aria-label="项目依赖图设置">
            <div className="graph-controls__body">
              {!embedded ? (
                <label className="select-field">
                  项目
                  <select
                    aria-label="依赖图项目"
                    value={selectedProjectPath}
                    onChange={(event) => selectProject(event.target.value)}
                  >
                    <option value="">选择项目</option>
                    {projectOptions.map((option) => (
                      <option key={option.project.path} value={option.project.path}>
                        {option.name}
                        {option.isWorkspace ? " · 工作区" : ""}
                        {option.isIgnored ? " · 已忽略" : ""}
                        {option.project.dependencyGraphSummary
                          ? ` · ${completenessLabel[option.project.dependencyGraphSummary.completeness]}`
                          : ""}
                      </option>
                    ))}
                  </select>
                </label>
              ) : null}
              <button
                className="button button--primary"
                onClick={() => void loadGraph()}
                disabled={!selectedProjectPath || isLoadingGraph}
              >
                {isLoadingGraph ? "解析中…" : "解析依赖图"}
              </button>
              <button
                className="button button--secondary"
                onClick={() => void exportSbom()}
                disabled={
                  !graph || graph.completeness === "unsupported" || graph.completeness === "invalid" || isExporting
                }
              >
                {isExporting ? "导出中…" : "导出 CycloneDX SBOM"}
              </button>
            </div>
            <div className="supply-chain-settings">
              {!embedded ? (
                <label className="checkbox-field">
                  <input
                    type="checkbox"
                    checked={showIgnoredProjects}
                    onChange={(event) => onToggleIgnoredProjects(event.target.checked)}
                  />
                  显示已忽略的生成目录
                </label>
              ) : null}
              <p>完整图按需读取当前锁文件，不写入 SQLite 快照。</p>
            </div>
          </section>
          {graph ? (
            <>
              <section className="metrics graph-metrics" aria-label="依赖图摘要">
                <div className="metric">
                  <div>
                    <strong>{graph.summary.nodeCount - 1}</strong>
                    <span>依赖节点</span>
                  </div>
                </div>
                <div className="metric">
                  <div>
                    <strong>{graph.summary.edgeCount}</strong>
                    <span>依赖关系</span>
                  </div>
                </div>
                <div className="metric">
                  <div>
                    <strong>{graph.summary.directCount}</strong>
                    <span>直接依赖</span>
                  </div>
                </div>
                <div className="metric">
                  <div>
                    <strong>{graph.summary.transitiveCount}</strong>
                    <span>传递依赖</span>
                  </div>
                </div>
              </section>
              <div className={`graph-completeness graph-completeness--${graph.completeness}`}>
                <strong>{completenessLabel[graph.completeness]}</strong>
                <span>
                  {graph.sources.join("、") || "无受支持锁文件"} · 摘要 {graph.sourceDigest || "—"}
                </span>
              </div>
              {graph.warnings.map((warning) => (
                <div className="inline-alert" key={warning}>
                  <Icon name="warning" />
                  <span>{warning}</span>
                </div>
              ))}
              <div className="toolbar" role="search">
                <label className="search-field">
                  <Icon name="search" />
                  <span className="sr-only">搜索图节点</span>
                  <input
                    value={graphQuery}
                    onChange={(event) => {
                      setGraphQuery(event.target.value);
                      setSelectedNodeId(undefined);
                    }}
                    placeholder="搜索图节点"
                  />
                </label>
                <label className="select-field">
                  范围
                  <select
                    aria-label="依赖图范围"
                    value={graphScope}
                    onChange={(event) => {
                      setGraphScope(event.target.value as typeof graphScope);
                      setSelectedNodeId(undefined);
                    }}
                  >
                    <option value="all">全部</option>
                    <option value="direct">直接依赖</option>
                    <option value="transitive">传递依赖</option>
                    <option value="duplicates">重复版本</option>
                  </select>
                </label>
                <span className="toolbar__count">{graphNodes.length} 个节点</span>
              </div>
              <div className="graph-layout">
                <section className="panel graph-node-panel">
                  <div className="panel__header">
                    <h2>依赖节点</h2>
                    <span className="count-label">{graphNodes.length}</span>
                  </div>
                  {graphNodes.length ? (
                    <div className="graph-node-list">
                      {graphNodes.map((node) => (
                        <button
                          className={selectedNodeId === node.id ? "graph-node graph-node--active" : "graph-node"}
                          key={node.id}
                          onClick={() => setSelectedNodeId(node.id)}
                        >
                          <span className="manager-chip">{node.ecosystem}</span>
                          <strong>{node.name}</strong>
                          <code>{node.version}</code>
                          {node.direct ? <small>直接</small> : <small>传递</small>}
                        </button>
                      ))}
                    </div>
                  ) : (
                    <EmptyState icon="dependencies" title="没有匹配的图节点" description="调整名称或依赖范围筛选。" />
                  )}
                </section>
                <section className="panel graph-detail-panel">
                  <div className="panel__header">
                    <h2>依赖路径</h2>
                  </div>
                  {selectedNode ? (
                    <div className="graph-detail">
                      <div className="graph-detail__title">
                        <span className="manager-chip">{selectedNode.ecosystem}</span>
                        <strong>{selectedNode.name}</strong>
                        <code>{selectedNode.version}</code>
                      </div>
                      <div>
                        <h3>从项目根到该依赖</h3>
                        <p className="dependency-path">
                          {selectedPath.map((node) => node.name).join(" → ") || "没有可达路径"}
                        </p>
                      </div>
                      <div className="graph-relations">
                        <section>
                          <h3>依赖它的节点</h3>
                          {incoming.length ? (
                            incoming.map((node) => (
                              <span key={node.id}>
                                {node.name} <code>{node.version}</code>
                              </span>
                            ))
                          ) : (
                            <p>没有上游节点</p>
                          )}
                        </section>
                        <section>
                          <h3>它依赖的节点</h3>
                          {outgoing.length ? (
                            outgoing.map((node) => (
                              <span key={node.id}>
                                {node.name} <code>{node.version}</code>
                              </span>
                            ))
                          ) : (
                            <p>没有下游节点</p>
                          )}
                        </section>
                      </div>
                    </div>
                  ) : (
                    <EmptyState
                      icon="dependencies"
                      title="选择一个依赖节点"
                      description="查看从项目根到该依赖的最短路径，以及正向和反向关系。"
                    />
                  )}
                </section>
              </div>
            </>
          ) : (
            <section className="panel">
              <EmptyState
                icon="dependencies"
                title="尚未解析项目依赖图"
                description="选择项目并按需读取当前锁文件；不会执行包管理器命令。"
              />
            </section>
          )}
        </>
      )}
    </>
  );
}

function buildDependencyUsageDisplays(
  usages: DependencyProjectUsage[],
  workspaces: ProjectWorkspace[],
  scanRoots: string[],
  ignoredDirectoryNames: string[],
  showIgnoredProjects: boolean,
): DependencyUsageDisplay[] {
  const workspaceByMemberPath = new Map(
    workspaces.flatMap((workspace) => [workspace.path, ...workspace.memberPaths].map((path) => [path, workspace])),
  );
  const grouped = new Map<string, DependencyUsageDisplay[]>();

  for (const usage of usages) {
    const workspace = workspaceByMemberPath.get(usage.projectPath);
    const isIgnored = isIgnoredScanPath(usage.projectPath, scanRoots, ignoredDirectoryNames);
    if (!showIgnoredProjects && isIgnored) continue;
    const key = workspace ? `workspace:${workspace.path}` : `project:${usage.projectPath}`;
    const bucket = grouped.get(key) ?? [];
    bucket.push({
      ...usage,
      key,
      isIgnored,
      isWorkspace: Boolean(workspace),
      memberCount: 1,
      projectName: workspace?.name ?? usage.projectName,
      projectPath: workspace?.path ?? usage.projectPath,
    });
    grouped.set(key, bucket);
  }

  return [...grouped.values()]
    .map((bucket) => {
      const representative = bucket[0];
      return {
        ...representative,
        memberCount: bucket.length,
        versionRequirement: uniqueJoined(bucket.map((item) => item.versionRequirement)),
        resolvedVersion:
          uniqueJoined(bucket.flatMap((item) => (item.resolvedVersion ? [item.resolvedVersion] : []))) || undefined,
        resolutionSource:
          uniqueJoined(bucket.flatMap((item) => (item.resolutionSource ? [item.resolutionSource] : []))) || undefined,
        scopes: [...new Set(bucket.flatMap((item) => item.scopes))].sort(),
        isIgnored: bucket.every((item) => item.isIgnored),
      };
    })
    .sort((left, right) => left.projectName.localeCompare(right.projectName));
}

function uniqueJoined(values: string[]): string {
  return [...new Set(values.filter(Boolean))].sort().join("、");
}

function relatedNodes(
  graph: ProjectDependencyGraph,
  nodeId: string,
  direction: "incoming" | "outgoing",
): DependencyGraphNode[] {
  const ids = new Set(
    graph.edges
      .filter((edge) => (direction === "outgoing" ? edge.from === nodeId : edge.to === nodeId))
      .map((edge) => (direction === "outgoing" ? edge.to : edge.from)),
  );
  return graph.nodes.filter((node) => ids.has(node.id));
}

function shortestDependencyPath(graph: ProjectDependencyGraph, targetId: string): DependencyGraphNode[] {
  const root = graph.nodes.find((node) => node.kind === "project");
  if (!root) return [];
  const adjacency = new Map<string, string[]>();
  for (const edge of graph.edges) {
    adjacency.set(edge.from, [...(adjacency.get(edge.from) ?? []), edge.to]);
  }
  const queue: Array<{ id: string; path: string[] }> = [{ id: root.id, path: [root.id] }];
  const visited = new Set<string>();
  while (queue.length) {
    const current = queue.shift();
    if (!current || visited.has(current.id)) continue;
    if (current.id === targetId)
      return current.path
        .map((id) => graph.nodes.find((node) => node.id === id))
        .filter((node): node is DependencyGraphNode => Boolean(node));
    visited.add(current.id);
    for (const id of adjacency.get(current.id) ?? []) {
      queue.push({ id, path: [...current.path, id] });
    }
  }
  return [];
}
