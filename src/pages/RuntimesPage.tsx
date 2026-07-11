import { useMemo, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import type { ExecutionTrust, RuntimeInstallation, RuntimeRequirementAssessment } from "../types";

const trustLabel: Record<ExecutionTrust, string> = {
  system: "系统路径",
  managed: "管理器路径",
  userManaged: "用户工具路径",
  unverified: "未经验证",
  notApplicable: "未执行",
};

export function RuntimesPage({ installations, assessments }: { installations: RuntimeInstallation[]; assessments: RuntimeRequirementAssessment[] }) {
  const [runtime, setRuntime] = useState("all");
  const [provider, setProvider] = useState("all");
  const [onlyIssues, setOnlyIssues] = useState(false);
  const runtimes = useMemo(() => [...new Set(installations.map((item) => item.runtime))].sort(), [installations]);
  const providers = useMemo(() => [...new Set(installations.map((item) => item.provider))].sort(), [installations]);
  const filteredInstallations = installations.filter((item) => (runtime === "all" || item.runtime === runtime) && (provider === "all" || item.provider === provider));
  const filteredAssessments = assessments.filter((item) => (runtime === "all" || item.runtime === runtime) && (!onlyIssues || item.status === "missing" || item.status === "mismatch"));
  const issueCount = assessments.filter((item) => item.status === "missing" || item.status === "mismatch").length;

  return (
    <>
      <PageHeader title="运行时" description="只读发现 Node.js、Python 与 Rust 安装来源，并关联项目声明；不会安装、切换或删除运行时。" />
      <section className="metrics runtime-metrics" aria-label="运行时摘要">
        <div className="metric"><span className="metric__icon"><Icon name="runtimes" /></span><div><strong>{installations.length}</strong><span>本机安装</span></div></div>
        <div className="metric"><span className="metric__icon"><Icon name="projects" /></span><div><strong>{assessments.length}</strong><span>项目声明</span></div></div>
        <div className="metric"><span className="metric__icon"><Icon name="warning" /></span><div><strong>{issueCount}</strong><span>需要关注</span></div></div>
      </section>
      <div className="toolbar" role="search">
        <label className="select-field">运行时<select aria-label="运行时类型" value={runtime} onChange={(event) => setRuntime(event.target.value)}><option value="all">全部</option>{runtimes.map((item) => <option key={item} value={item}>{item}</option>)}</select></label>
        <label className="select-field">提供者<select aria-label="运行时提供者" value={provider} onChange={(event) => setProvider(event.target.value)}><option value="all">全部</option>{providers.map((item) => <option key={item} value={item}>{item}</option>)}</select></label>
        <label className="checkbox-field"><input type="checkbox" checked={onlyIssues} onChange={(event) => setOnlyIssues(event.target.checked)} />仅看关联异常</label>
      </div>
      <div className="runtime-layout">
        <section className="panel">
          <div className="panel__header"><h2>本机安装</h2><span className="count-label">{filteredInstallations.length}</span></div>
          {filteredInstallations.length ? <div className="runtime-installations">{filteredInstallations.map((item) => <article className="runtime-installation" key={item.id}><div><span className="manager-chip">{item.runtime}</span>{item.isActive ? <span className="runtime-active">当前</span> : null}<h3>{item.version}</h3><code title={item.path}>{item.path}</code></div><div><strong>{item.provider}</strong><span>{trustLabel[item.executionTrust]}</span></div></article>)}</div> : <EmptyState icon="runtimes" title="没有匹配的运行时" description="调整运行时或提供者筛选条件。" />}
        </section>
        <section className="panel">
          <div className="panel__header"><h2>项目关联</h2><span className="count-label">{filteredAssessments.length}</span></div>
          {filteredAssessments.length ? <div className="runtime-assessments">{filteredAssessments.map((item) => <article className={`runtime-assessment runtime-assessment--${item.status}`} key={`${item.projectPath}:${item.runtime}`}><div><strong>{item.projectName}</strong><code title={item.projectPath}>{item.projectPath}</code></div><div><span>{item.runtime} {item.requirement}</span><b>当前 {item.activeVersion ?? "未发现"}</b><small>{item.message}</small></div></article>)}</div> : <EmptyState icon="check" title="没有关联异常" description="当前筛选范围内的项目运行时声明均有本机安装可供参考。" />}
        </section>
      </div>
    </>
  );
}
