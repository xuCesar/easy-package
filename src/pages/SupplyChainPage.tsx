import { useMemo, useRef, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { apiErrorMessage } from "../lib/apiError";
import { buildProjectAnalysisOptions } from "../lib/projectAnalysisOptions";
import type {
  ProjectMetadata,
  ProjectSupplyChainReport,
  ProjectWorkspace,
  ReportExportResult,
  SupplyChainRiskFinding,
} from "../types";

interface SupplyChainPageProps {
  embedded?: boolean;
  projects: ProjectMetadata[];
  workspaces: ProjectWorkspace[];
  scanRoots: string[];
  ignoredDirectoryNames: string[];
  projectPath: string;
  onSelectProject: (path: string) => void;
  showIgnoredProjects: boolean;
  onToggleIgnoredProjects: (show: boolean) => void;
  onLoadReport: (projectPath: string) => Promise<ProjectSupplyChainReport>;
  onExportSbom: (projectPath: string) => Promise<ReportExportResult>;
}

const ruleLabels: Record<string, string> = {
  LOCKFILE_MISSING: "缺少完整锁文件",
  DEPENDENCY_GRAPH_PARTIAL: "依赖图不完整",
  DEPENDENCY_GRAPH_INVALID: "锁文件无效",
  MULTIPLE_RESOLVED_VERSIONS: "多个解析版本",
  LOCKFILE_ENTRY_UNREACHABLE: "不可达锁文件条目",
  DEPENDENCY_CYCLE_DETECTED: "依赖循环",
  NON_REGISTRY_DEPENDENCY: "本地或工作区依赖",
  PACKAGE_SOURCE_UNKNOWN: "来源未知",
  DEPENDENCY_VERSION_MISSING: "版本缺失",
};

export function SupplyChainPage({
  embedded = false,
  projects,
  workspaces,
  scanRoots,
  ignoredDirectoryNames,
  projectPath,
  onSelectProject,
  showIgnoredProjects,
  onToggleIgnoredProjects,
  onLoadReport,
  onExportSbom,
}: SupplyChainPageProps) {
  const projectOptions = useMemo(
    () => buildProjectAnalysisOptions(projects, workspaces, scanRoots, ignoredDirectoryNames, showIgnoredProjects),
    [ignoredDirectoryNames, projects, scanRoots, showIgnoredProjects, workspaces],
  );
  const [report, setReport] = useState<ProjectSupplyChainReport>();
  const [query, setQuery] = useState("");
  const [severity, setSeverity] = useState<"all" | "warning" | "info">("all");
  const [rule, setRule] = useState("all");
  const [selectedFindingId, setSelectedFindingId] = useState<string>();
  const [isLoading, setIsLoading] = useState(false);
  const [isExporting, setIsExporting] = useState(false);
  const [error, setError] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const reportRequestToken = useRef(0);

  const aggregate = useMemo(
    () =>
      [
        ...new Map(
          projectOptions.flatMap((option) => option.projects).map((project) => [project.path, project]),
        ).values(),
      ].reduce(
        (summary, project) => {
          const riskSummary = project.supplyChainRiskSummary;
          summary.projects += riskSummary ? 1 : 0;
          summary.total += riskSummary?.totalCount ?? 0;
          summary.warnings += riskSummary?.warningCount ?? 0;
          return summary;
        },
        { projects: 0, total: 0, warnings: 0 },
      ),
    [projectOptions],
  );
  const rules = useMemo(() => report?.summary.ruleIds ?? [], [report]);
  const filtered = useMemo(
    () =>
      report?.findings.filter((finding) => {
        const normalized = query.trim().toLowerCase();
        const matchesQuery =
          !normalized ||
          [finding.title, finding.description, finding.code, ...finding.evidence].some((value) =>
            value.toLowerCase().includes(normalized),
          );
        return (
          matchesQuery &&
          (severity === "all" || finding.severity === severity) &&
          (rule === "all" || finding.code === rule)
        );
      }) ?? [],
    [query, report, rule, severity],
  );
  const selectedFinding = report?.findings.find((finding) => finding.id === selectedFindingId);

  const selectProject = (path: string) => {
    reportRequestToken.current += 1;
    onSelectProject(path);
    setReport(undefined);
    setSelectedFindingId(undefined);
  };

  const loadReport = async () => {
    if (!projectPath) return;
    reportRequestToken.current += 1;
    const token = reportRequestToken.current;
    setIsLoading(true);
    setError(undefined);
    setNotice(undefined);
    setSelectedFindingId(undefined);
    try {
      const report = await onLoadReport(projectPath);
      if (reportRequestToken.current !== token) return;
      setReport(report);
    } catch (loadError) {
      if (reportRequestToken.current !== token) return;
      setReport(undefined);
      setError(messageFrom(loadError));
    } finally {
      if (reportRequestToken.current === token) {
        setIsLoading(false);
      }
    }
  };

  const exportSbom = async () => {
    if (!report) return;
    setIsExporting(true);
    setError(undefined);
    setNotice(undefined);
    try {
      const result = await onExportSbom(report.projectPath);
      setNotice(result.saved ? "CycloneDX SBOM 已导出。" : "已取消导出。");
    } catch (exportError) {
      setError(messageFrom(exportError));
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <>
      {error ? (
        <div className="inline-alert inline-alert--error">
          <Icon name="warning" />
          <span>{error}</span>
        </div>
      ) : null}
      {notice ? (
        <div className="inline-alert">
          <Icon name="info" />
          <span>{notice}</span>
        </div>
      ) : null}
      {!embedded ? (
        <section className="metrics supply-chain-metrics" aria-label="锁文件问题摘要">
          <div className="metric">
            <span className="metric__icon">
              <Icon name="projects" />
            </span>
            <div>
              <strong>{aggregate.projects}</strong>
              <span>有问题摘要的项目</span>
            </div>
          </div>
          <div className="metric">
            <span className="metric__icon">
              <Icon name="warning" />
            </span>
            <div>
              <strong>{aggregate.warnings}</strong>
              <span>警告项</span>
            </div>
          </div>
          <div className="metric">
            <span className="metric__icon">
              <Icon name="info" />
            </span>
            <div>
              <strong>{aggregate.total}</strong>
              <span>结构性发现</span>
            </div>
          </div>
        </section>
      ) : null}
      <section className="panel graph-controls" aria-label="锁文件问题分析设置">
        <div className="graph-controls__body">
          {!embedded ? (
            <label className="select-field">
              项目
              <select
                aria-label="锁文件问题项目"
                value={projectPath}
                onChange={(event) => selectProject(event.target.value)}
              >
                <option value="">选择项目</option>
                {projectOptions.map((option) => (
                  <option key={option.project.path} value={option.project.path}>
                    {option.name}
                    {option.isWorkspace ? " · 工作区" : ""}
                    {option.isIgnored ? " · 已忽略" : ""}
                    {option.supplyChainRiskSummary ? ` · ${option.supplyChainRiskSummary.warningCount} 个警告` : ""}
                  </option>
                ))}
              </select>
            </label>
          ) : null}
          <button
            className="button button--primary"
            onClick={() => void loadReport()}
            disabled={!projectPath || isLoading}
          >
            {isLoading ? "检查中…" : "检查锁文件问题"}
          </button>
          <button
            className="button button--secondary"
            onClick={() => void exportSbom()}
            disabled={!report || isExporting}
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
          <p>完整证据按需从当前锁文件重建，不写入 SQLite；结果不包含漏洞或许可证结论。</p>
        </div>
      </section>
      {report ? (
        <>
          <section className="metrics graph-metrics" aria-label="当前项目锁文件问题摘要">
            <div className="metric">
              <div>
                <strong>{report.summary.totalCount}</strong>
                <span>全部发现</span>
              </div>
            </div>
            <div className="metric">
              <div>
                <strong>{report.summary.warningCount}</strong>
                <span>警告</span>
              </div>
            </div>
            <div className="metric">
              <div>
                <strong>{report.summary.infoCount}</strong>
                <span>提示</span>
              </div>
            </div>
            <div className="metric">
              <div>
                <strong>{report.summary.ruleIds.length}</strong>
                <span>触发规则</span>
              </div>
            </div>
          </section>
          <div className="toolbar" role="search">
            <label className="search-field">
              <Icon name="search" />
              <span className="sr-only">搜索锁文件问题</span>
              <input
                value={query}
                onChange={(event) => {
                  setQuery(event.target.value);
                  setSelectedFindingId(undefined);
                }}
                placeholder="搜索问题、规则或证据"
              />
            </label>
            <label className="select-field">
              级别
              <select
                aria-label="问题级别"
                value={severity}
                onChange={(event) => {
                  setSeverity(event.target.value as typeof severity);
                  setSelectedFindingId(undefined);
                }}
              >
                <option value="all">全部级别</option>
                <option value="warning">警告</option>
                <option value="info">提示</option>
              </select>
            </label>
            <label className="select-field">
              规则
              <select
                aria-label="问题规则"
                value={rule}
                onChange={(event) => {
                  setRule(event.target.value);
                  setSelectedFindingId(undefined);
                }}
              >
                <option value="all">全部规则</option>
                {rules.map((item) => (
                  <option key={item} value={item}>
                    {ruleLabels[item] ?? item}
                  </option>
                ))}
              </select>
            </label>
            <span className="toolbar__count">{filtered.length} 个结果</span>
          </div>
          <div className="supply-chain-layout">
            <section className="panel risk-list-panel">
              <div className="panel__header">
                <h2>结构性发现</h2>
                <span className="count-label">{filtered.length}</span>
              </div>
              {filtered.length ? (
                <div className="risk-list">
                  {filtered.map((finding) => (
                    <FindingButton
                      key={finding.id}
                      finding={finding}
                      selected={finding.id === selectedFindingId}
                      onSelect={() => setSelectedFindingId(finding.id)}
                    />
                  ))}
                </div>
              ) : (
                <EmptyState icon="dependencies" title="没有匹配的问题" description="调整搜索、级别或规则筛选。" />
              )}
            </section>
            <section className="panel risk-detail-panel">
              <div className="panel__header">
                <h2>证据与依赖路径</h2>
              </div>
              {selectedFinding ? (
                <div className="risk-detail">
                  <div>
                    <span className={`risk-severity risk-severity--${selectedFinding.severity}`}>
                      {selectedFinding.severity === "warning" ? "警告" : "提示"}
                    </span>
                    <code>{selectedFinding.code}</code>
                  </div>
                  <h3>{selectedFinding.title}</h3>
                  <p>{selectedFinding.description}</p>
                  <section>
                    <h3>依赖路径</h3>
                    <p className="dependency-path">
                      {selectedFinding.dependencyPath.join(" → ") || "项目级发现，无单一依赖路径"}
                    </p>
                  </section>
                  <section>
                    <h3>本机证据</h3>
                    {selectedFinding.evidence.length ? (
                      <ul>
                        {selectedFinding.evidence.map((item) => (
                          <li key={item}>
                            <code>{item}</code>
                          </li>
                        ))}
                      </ul>
                    ) : (
                      <p>没有附加证据。</p>
                    )}
                  </section>
                  <p className="risk-disclaimer">本结果仅基于离线结构规则，不代表已确认存在安全漏洞或许可证问题。</p>
                </div>
              ) : (
                <EmptyState
                  icon="dependencies"
                  title="选择一个问题项"
                  description="查看稳定规则 ID、本机证据和从项目根开始的依赖路径。"
                />
              )}
            </section>
          </div>
        </>
      ) : (
        <section className="panel">
          <EmptyState
            icon="dependencies"
            title="尚未检查锁文件问题"
            description="选择项目后按需读取当前锁文件；不会联网或修改项目。"
          />
        </section>
      )}
    </>
  );
}

function FindingButton({
  finding,
  selected,
  onSelect,
}: {
  finding: SupplyChainRiskFinding;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button className={selected ? "risk-item risk-item--active" : "risk-item"} onClick={onSelect}>
      <span className={`risk-severity risk-severity--${finding.severity}`}>
        {finding.severity === "warning" ? "警告" : "提示"}
      </span>
      <span>
        <strong>{finding.title}</strong>
        <code>{finding.code}</code>
        <small>{finding.evidence[0] ?? "项目级发现"}</small>
      </span>
    </button>
  );
}

function messageFrom(error: unknown): string {
  return apiErrorMessage(error, "锁文件问题检查失败，请重试。");
}
