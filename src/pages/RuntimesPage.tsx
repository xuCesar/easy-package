import { useMemo, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import type { ExecutionTrust, RuntimeInstallation } from "../types";

const trustLabel: Record<ExecutionTrust, string> = {
  system: "系统路径",
  managed: "管理器路径",
  userManaged: "用户工具路径",
  unverified: "未经验证",
  notApplicable: "未执行",
};

export function RuntimesPage({ installations }: { installations: RuntimeInstallation[] }) {
  const [runtime, setRuntime] = useState("all");
  const [provider, setProvider] = useState("all");
  const runtimes = useMemo(() => [...new Set(installations.map((item) => item.runtime))].sort(), [installations]);
  const providers = useMemo(() => [...new Set(installations.map((item) => item.provider))].sort(), [installations]);
  const filteredInstallations = installations.filter(
    (item) => (runtime === "all" || item.runtime === runtime) && (provider === "all" || item.provider === provider),
  );
  return (
    <>
      <PageHeader
        title="运行时"
        description="只读发现本机 Node.js、Python 与 Rust 安装来源；不会安装、切换或删除运行时。项目匹配结果在对应项目详情中查看。"
      />
      <section className="metrics runtime-metrics" aria-label="运行时摘要">
        <div className="metric">
          <span className="metric__icon">
            <Icon name="runtimes" />
          </span>
          <div>
            <strong>{installations.length}</strong>
            <span>本机安装</span>
          </div>
        </div>
        <div className="metric">
          <span className="metric__icon">
            <Icon name="projects" />
          </span>
          <div>
            <strong>{runtimes.length}</strong>
            <span>运行时类型</span>
          </div>
        </div>
        <div className="metric">
          <span className="metric__icon">
            <Icon name="environment" />
          </span>
          <div>
            <strong>{providers.length}</strong>
            <span>安装来源</span>
          </div>
        </div>
      </section>
      <div className="toolbar" role="search">
        <label className="select-field">
          运行时
          <select aria-label="运行时类型" value={runtime} onChange={(event) => setRuntime(event.target.value)}>
            <option value="all">全部</option>
            {runtimes.map((item) => (
              <option key={item} value={item}>
                {item}
              </option>
            ))}
          </select>
        </label>
        <label className="select-field">
          提供者
          <select aria-label="运行时提供者" value={provider} onChange={(event) => setProvider(event.target.value)}>
            <option value="all">全部</option>
            {providers.map((item) => (
              <option key={item} value={item}>
                {item}
              </option>
            ))}
          </select>
        </label>
      </div>
      <section className="panel">
        <div className="panel__header">
          <h2>本机安装</h2>
          <span className="count-label">{filteredInstallations.length}</span>
        </div>
        {filteredInstallations.length ? (
          <div className="runtime-installations">
            {filteredInstallations.map((item) => (
              <article className="runtime-installation" key={item.id}>
                <div>
                  <div className="runtime-installation__heading">
                    <span className="manager-chip">{item.runtime}</span>
                    {item.isActive ? <span className="runtime-active">当前</span> : null}
                    <h3>{item.version}</h3>
                  </div>
                  <code title={item.path}>{item.path}</code>
                </div>
                <div>
                  <strong>{item.provider}</strong>
                  <span>{trustLabel[item.executionTrust]}</span>
                </div>
              </article>
            ))}
          </div>
        ) : (
          <EmptyState icon="runtimes" title="没有匹配的运行时" description="调整运行时或提供者筛选条件。" />
        )}
      </section>
    </>
  );
}
