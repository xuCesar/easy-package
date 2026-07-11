import { useMemo, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import type { ManagedPackage, PackageAction, PackageActionAuditRecord, PackageActionPlan, PackageActionProgress, PackageActionResult } from "../types";

interface ActionCenterPageProps {
  packages: ManagedPackage[];
  plan?: PackageActionPlan;
  result?: PackageActionResult;
  audit: PackageActionAuditRecord[];
  progress: PackageActionProgress[];
  isPlanning: boolean;
  isExecuting: boolean;
  error?: string;
  onCreatePlan: (action: PackageAction, targets: string[]) => Promise<PackageActionPlan | undefined>;
  onExecute: () => Promise<void>;
  onCancel: () => Promise<void>;
  onClearPlan: () => void;
}

const actionLabels: Record<PackageAction, string> = {
  install: "安装",
  upgrade: "升级",
  uninstall: "卸载",
  cleanup: "缓存清理",
};

const statusLabels = {
  planned: "待确认",
  running: "执行中",
  succeeded: "成功",
  failed: "失败",
  unknown: "状态未知",
};

export function ActionCenterPage(props: ActionCenterPageProps) {
  const [action, setAction] = useState<PackageAction>("install");
  const [installTarget, setInstallTarget] = useState("");
  const [upgradeTargets, setUpgradeTargets] = useState<string[]>([]);
  const [uninstallTarget, setUninstallTarget] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const formulas = useMemo(() => [...props.packages.filter((pkg) => pkg.managerId === "homebrew")].sort((left, right) => left.name.localeCompare(right.name)), [props.packages]);
  const upgradeCandidates = formulas;
  const canCancel = props.progress.at(-1)?.cancellable === true;
  const targets = action === "install" ? [installTarget.trim()].filter(Boolean) : action === "upgrade" ? upgradeTargets : action === "uninstall" ? [uninstallTarget].filter(Boolean) : [];

  const changeAction = (next: PackageAction) => {
    setAction(next);
    setConfirmed(false);
    props.onClearPlan();
  };

  const createPlan = async () => {
    setConfirmed(false);
    await props.onCreatePlan(action, targets);
  };

  const execute = async () => {
    setConfirmed(false);
    await props.onExecute();
  };

  return (
    <>
      <PageHeader title="操作中心" description="通过后端白名单执行 Homebrew Formula 操作；每次修改都先预检、确认，并在完成后重新扫描。" />
      <div className="write-boundary-banner"><Icon name="warning" /><div><strong>受控写入模式</strong><span>不使用 Shell、不接受自定义参数、不请求 sudo；取消或超时后状态会标记为未知。</span></div></div>
      {props.error ? <div className="inline-alert inline-alert--error"><Icon name="warning" /><span>{props.error}</span></div> : null}
      <div className="view-tabs action-tabs" role="tablist" aria-label="软件包操作">
        {(Object.keys(actionLabels) as PackageAction[]).map((item) => <button key={item} role="tab" aria-selected={action === item} className={action === item ? "view-tab view-tab--active" : "view-tab"} onClick={() => changeAction(item)} disabled={props.isExecuting}>{actionLabels[item]}</button>)}
      </div>
      <section className="panel action-builder" aria-label="操作预检设置">
        <div className="panel__header"><h2>{actionLabels[action]} Homebrew Formula</h2><span className="readonly-label">Homebrew</span></div>
        <div className="action-builder__body">
          {action === "install" ? <label className="action-input">Formula 名称<input aria-label="待安装 Formula" value={installTarget} onChange={(event) => setInstallTarget(event.target.value)} placeholder="例如 jq 或 user/tap/formula" disabled={props.isExecuting} /><small>名称会经过严格语法校验；生成计划不会联网，存在性由确认执行后的 Homebrew 验证。</small></label> : null}
          {action === "upgrade" ? <div className="formula-selection"><div><strong>选择待升级 Formula</strong><small>最多选择 20 个已安装 Formula；更新状态未知时，由 Homebrew 在执行阶段判断是否需要升级。</small></div>{upgradeCandidates.length ? <div className="formula-checks">{upgradeCandidates.map((pkg) => <label key={pkg.id}><input type="checkbox" checked={upgradeTargets.includes(pkg.name)} onChange={(event) => setUpgradeTargets((current) => event.target.checked ? [...current, pkg.name] : current.filter((name) => name !== pkg.name))} disabled={props.isExecuting} /><span><strong>{pkg.name}</strong><code>{pkg.latestVersion ? `${pkg.version} → ${pkg.latestVersion}` : `${pkg.version} · 更新状态未知`}</code></span></label>)}</div> : <EmptyState icon="packages" title="没有已安装的 Homebrew Formula" description="先刷新环境扫描，或选择其他操作。" />}</div> : null}
          {action === "uninstall" ? <label className="action-input">已安装 Formula<select aria-label="待卸载 Formula" value={uninstallTarget} onChange={(event) => setUninstallTarget(event.target.value)} disabled={props.isExecuting}><option value="">选择 Formula</option>{formulas.map((pkg) => <option key={pkg.id} value={pkg.name}>{pkg.name} · {pkg.version}</option>)}</select><small>预检会调用 `brew uses --installed` 展示被依赖关系；Homebrew 仍可能拒绝不安全卸载。</small></label> : null}
          {action === "cleanup" ? <div className="cleanup-description"><Icon name="packages" /><div><strong>预览 Homebrew 缓存清理</strong><p>先执行 `brew cleanup --dry-run`。确认后仅调用 `brew cleanup`，不会直接递归删除缓存目录。</p></div></div> : null}
          <button className="button button--primary" onClick={() => void createPlan()} disabled={props.isPlanning || props.isExecuting || (action !== "cleanup" && targets.length === 0)}>{props.isPlanning ? "正在预检…" : "生成操作计划"}</button>
        </div>
      </section>
      {props.plan ? <section className="panel action-plan" aria-label="待确认操作计划">
        <div className="panel__header"><h2>待确认操作计划</h2><span className="action-status action-status--planned">待确认</span></div>
        <div className="action-plan__body">
          <div className="action-plan__summary"><div><span>操作</span><strong>{actionLabels[props.plan.action]}</strong></div><div><span>目标</span><strong>{props.plan.targets.join("、") || "Homebrew 缓存"}</strong></div><div><span>网络</span><strong>{props.plan.requiresNetwork ? "需要访问 Homebrew 源" : "不主动联网"}</strong></div></div>
          <div><h3>固定命令预览</h3><code className="command-preview">{props.plan.commandPreview}</code></div>
          <div><h3>预检结果</h3><ul className="plan-lines">{props.plan.previewLines.map((line) => <li key={line}>{line}</li>)}</ul></div>
          <div><h3>风险提示</h3><ul className="plan-warnings">{props.plan.warnings.map((warning) => <li key={warning}><Icon name="warning" />{warning}</li>)}</ul></div>
          <label className="confirmation-field"><input type="checkbox" checked={confirmed} onChange={(event) => setConfirmed(event.target.checked)} /><span>我已核对目标、固定命令和风险，并确认修改本机 Homebrew 环境。</span></label>
          <div className="action-plan__actions"><button className="button button--primary" onClick={() => void execute()} disabled={!confirmed || props.isExecuting}>确认并执行</button><button className="button button--secondary" onClick={props.onClearPlan} disabled={props.isExecuting}>放弃计划</button></div>
        </div>
      </section> : null}
      {props.isExecuting || props.progress.length ? <section className="panel action-progress-panel" aria-label="操作进度">
        <div className="panel__header"><h2>操作进度</h2>{props.isExecuting ? <span className="scan-indicator action-spinner"><Icon name="refresh" /></span> : null}</div>
        <div className="action-progress-list">{props.progress.map((item, index) => <div key={`${item.timestamp}-${index}`}><time>{new Date(item.timestamp).toLocaleTimeString()}</time><span>{item.message}</span></div>)}</div>
        {props.isExecuting ? <div className="action-progress-panel__actions"><button className="button button--danger" onClick={() => void props.onCancel()} disabled={!canCancel}>终止操作</button><p>{canCancel ? "终止不会回滚已完成步骤；应用会强制重新扫描并标记实际结果。" : "正在完成强制复扫，此阶段不能取消。"}</p></div> : null}
      </section> : null}
      {props.result ? <section className={`panel action-result action-result--${props.result.status}`} aria-label="操作结果">
        <div className="panel__header"><h2>操作结果</h2><span className={`action-status action-status--${props.result.status}`}>{statusLabels[props.result.status]}</span></div>
        <div className="action-result__body"><p>{props.result.error ?? "Homebrew 操作成功，环境扫描已更新。"}</p>{props.result.comparison ? <div className="result-changes"><strong>{props.result.comparison.changes.length} 项环境变化</strong>{props.result.comparison.changes.map((change) => <div key={`${change.entity}:${change.key}`}><span>{change.title}</span><small>{change.description}</small></div>)}</div> : <p>没有可比较的前后快照。</p>}</div>
      </section> : null}
      <section className="panel action-audit" aria-label="操作审计记录">
        <div className="panel__header"><h2>最近操作审计</h2><span className="count-label">{props.audit.length}</span></div>
        {props.audit.length ? <div className="audit-list">{props.audit.map((record) => <article key={record.actionId}><span className={`action-status action-status--${record.status}`}>{statusLabels[record.status]}</span><div><strong>{actionLabels[record.action]} · {record.targets.join("、") || "Homebrew 缓存"}</strong><code>{record.commandPreview}</code></div><time>{new Date(record.finishedAt).toLocaleString()}</time></article>)}</div> : <EmptyState icon="history" title="还没有写操作记录" description="完成首个受控操作后，结果会保存在本机审计记录中。" />}
      </section>
    </>
  );
}
