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
  const [selectedRuntime, setSelectedRuntime] = useState<string>();
  const runtimes = useMemo(() => [...new Set(installations.map((item) => item.runtime))].sort(), [installations]);
  const providers = useMemo(() => [...new Set(installations.map((item) => item.provider))].sort(), [installations]);
  const filteredInstallations = useMemo(
    () =>
      installations.filter(
        (item) => (runtime === "all" || item.runtime === runtime) && (provider === "all" || item.provider === provider),
      ),
    [installations, provider, runtime],
  );
  const runtimeGroups = useMemo(() => groupInstallations(filteredInstallations), [filteredInstallations]);
  const selectedInstallations = useMemo(
    () => (selectedRuntime ? installations.filter((item) => item.runtime === selectedRuntime) : []),
    [installations, selectedRuntime],
  );

  if (selectedRuntime && selectedInstallations.length > 1) {
    return (
      <RuntimeVersionsPage
        runtime={selectedRuntime}
        installations={selectedInstallations}
        onBack={() => setSelectedRuntime(undefined)}
      />
    );
  }

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
          <h2>当前版本</h2>
          <span className="count-label">{runtimeGroups.length}</span>
        </div>
        {runtimeGroups.length ? (
          <div className="runtime-installations">
            {runtimeGroups.map(({ runtime: runtimeName, current, versions }) => (
              <article className="runtime-installation" key={runtimeName}>
                <div>
                  <div className="runtime-installation__heading">
                    <span className="manager-chip">{runtimeName}</span>
                    {current.isActive ? <span className="runtime-active">当前</span> : null}
                    <h3>{current.version}</h3>
                  </div>
                  <code title={current.path}>{current.path}</code>
                </div>
                <div>
                  <strong>{current.provider}</strong>
                  <span>{trustLabel[current.executionTrust]}</span>
                  {versions.length > 1 ? (
                    <button
                      className="text-button runtime-installation__versions"
                      onClick={() => setSelectedRuntime(runtimeName)}
                      aria-label={`查看 ${runtimeName} 的其他版本`}
                    >
                      查看其他版本 ({versions.length - 1}) <Icon name="chevron" />
                    </button>
                  ) : null}
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

interface RuntimeGroup {
  runtime: string;
  current: RuntimeInstallation;
  versions: RuntimeInstallation[];
}

function groupInstallations(installations: RuntimeInstallation[]): RuntimeGroup[] {
  const groups = new Map<string, RuntimeInstallation[]>();
  for (const installation of installations) {
    const versions = groups.get(installation.runtime) ?? [];
    versions.push(installation);
    groups.set(installation.runtime, versions);
  }
  return [...groups.entries()]
    .map(([runtime, versions]) => ({
      runtime,
      current: versions.find((item) => item.isActive) ?? versions[0],
      versions,
    }))
    .sort((left, right) => left.runtime.localeCompare(right.runtime));
}

function RuntimeVersionsPage({
  runtime,
  installations,
  onBack,
}: {
  runtime: string;
  installations: RuntimeInstallation[];
  onBack: () => void;
}) {
  return (
    <>
      <PageHeader
        title={`${runtime} 版本`}
        description="查看该运行时在本机发现的全部安装；页面只读，不会切换或修改运行时。"
        actions={
          <button className="button button--secondary" onClick={onBack}>
            返回运行时
          </button>
        }
      />
      <section className="panel">
        <div className="panel__header">
          <h2>全部版本</h2>
          <span className="count-label">{installations.length}</span>
        </div>
        <div className="runtime-installations">
          {installations.map((item) => (
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
      </section>
    </>
  );
}
