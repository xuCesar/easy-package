import { useMemo, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import type { ManagedPackage, PackageAction, PackageActionAuditRecord, PackageActionPlan, PackageActionProgress, PackageActionResult } from "../types";

type WritableManagerId = "homebrew" | "pnpm";

interface ActionCenterPageProps {
  packages: ManagedPackage[];
  plan?: PackageActionPlan;
  result?: PackageActionResult;
  audit: PackageActionAuditRecord[];
  progress: PackageActionProgress[];
  isPlanning: boolean;
  isExecuting: boolean;
  error?: string;
  onCreatePlan: (managerId: WritableManagerId, action: PackageAction, targets: string[]) => Promise<PackageActionPlan | undefined>;
  onExecute: () => Promise<void>;
  onCancel: () => Promise<void>;
  onClearPlan: () => void;
}

const actionLabels: Record<PackageAction, string> = { install: "安装", upgrade: "升级", uninstall: "卸载", cleanup: "缓存清理" };
const managerLabels: Record<WritableManagerId, string> = { homebrew: "Homebrew", pnpm: "pnpm" };
const statusLabels = { planned: "待确认", running: "可能中断", succeeded: "成功", failed: "失败", unknown: "状态未知" };

export function ActionCenterPage(props: ActionCenterPageProps) {
  const [managerId, setManagerId] = useState<WritableManagerId>("homebrew");
  const [action, setAction] = useState<PackageAction>("install");
  const [installTarget, setInstallTarget] = useState("");
  const [upgradeTargets, setUpgradeTargets] = useState<string[]>([]);
  const [uninstallTarget, setUninstallTarget] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [auditManager, setAuditManager] = useState<"all" | WritableManagerId>("all");
  const packages = useMemo(() => [...props.packages.filter((pkg) => pkg.managerId === managerId)].sort((left, right) => left.name.localeCompare(right.name)), [managerId, props.packages]);
  const filteredAudit = useMemo(() => props.audit.filter((record) => auditManager === "all" || record.managerId === auditManager), [auditManager, props.audit]);
  const recoveryRequired = props.audit.some((record) => record.status === "running");
  const canCancel = props.progress.at(-1)?.cancellable === true;
  const targets = action === "install" ? [installTarget.trim()].filter(Boolean) : action === "upgrade" ? upgradeTargets : action === "uninstall" ? [uninstallTarget].filter(Boolean) : [];
  const managerName = managerLabels[managerId];
  const subjectName = managerId === "homebrew" ? "Formula" : "全局包";

  const resetPlan = () => {
    setConfirmed(false);
    props.onClearPlan();
  };
  const changeManager = (next: WritableManagerId) => {
    setManagerId(next);
    setInstallTarget("");
    setUpgradeTargets([]);
    setUninstallTarget("");
    resetPlan();
  };
  const changeAction = (next: PackageAction) => {
    setAction(next);
    resetPlan();
  };
  const createPlan = async () => {
    setConfirmed(false);
    await props.onCreatePlan(managerId, action, targets);
  };

  return (
    <>
      <PageHeader title="操作中心" description="通过后端固定白名单执行 Homebrew Formula 与 pnpm 全局包操作；每次修改都先预检、确认并在完成后重新扫描。" />
      <div className="write-boundary-banner"><Icon name="warning" /><div><strong>受控写入模式</strong><span>不使用 Shell、不接受自定义参数、不请求 sudo；pnpm 固定禁用 lifecycle scripts。</span></div></div>
      {recoveryRequired ? <div className="inline-alert inline-alert--error"><Icon name="warning" /><span>上次操作可能中断。请先在概览页完成一次环境扫描，确认实际状态后才能继续写操作。</span></div> : null}
      {props.error ? <div className="inline-alert inline-alert--error"><Icon name="warning" /><span>{props.error}</span></div> : null}

      <div className="view-tabs action-manager-tabs" role="tablist" aria-label="写操作包管理器">
        {(["homebrew", "pnpm"] as const).map((item) => <button key={item} role="tab" aria-selected={managerId === item} className={managerId === item ? "view-tab view-tab--active" : "view-tab"} onClick={() => changeManager(item)} disabled={props.isExecuting}>{managerLabels[item]}</button>)}
      </div>
      <div className="view-tabs action-tabs" role="tablist" aria-label="软件包操作">
        {(Object.keys(actionLabels) as PackageAction[]).map((item) => <button key={item} role="tab" aria-selected={action === item} className={action === item ? "view-tab view-tab--active" : "view-tab"} onClick={() => changeAction(item)} disabled={props.isExecuting}>{actionLabels[item]}</button>)}
      </div>

      <section className="panel action-builder" aria-label="操作预检设置">
        <div className="panel__header"><h2>{actionLabels[action]} {managerName} {subjectName}</h2><span className={`manager-chip manager-chip--${managerId}`}>{managerName}</span></div>
        <div className="action-builder__body">
          {action === "install" ? <label className="action-input">{subjectName} 名称<input aria-label={`待安装${managerId === "homebrew" ? " Formula" : " pnpm 全局包"}`} value={installTarget} onChange={(event) => setInstallTarget(event.target.value)} placeholder={managerId === "homebrew" ? "例如 jq 或 user/tap/formula" : "例如 eslint 或 @scope/tool"} disabled={props.isExecuting} /><small>{managerId === "homebrew" ? "名称会经过严格语法校验，存在性由 Homebrew 执行时验证。" : "仅接受普通包名或 @scope/name；拒绝版本、URL、Git 和本地路径来源。"}</small></label> : null}
          {action === "upgrade" ? <div className="formula-selection"><div><strong>选择待升级{subjectName}</strong><small>最多选择 20 个当前扫描到的已安装包。</small></div>{packages.length ? <div className="formula-checks">{packages.map((pkg) => <label key={pkg.id}><input type="checkbox" checked={upgradeTargets.includes(pkg.name)} onChange={(event) => setUpgradeTargets((current) => event.target.checked ? [...current, pkg.name] : current.filter((name) => name !== pkg.name))} disabled={props.isExecuting} /><span><strong>{pkg.name}</strong><code>{pkg.latestVersion ? `${pkg.version} → ${pkg.latestVersion}` : `${pkg.version} · 更新状态未知`}</code></span></label>)}</div> : <EmptyState icon="packages" title={`没有已安装的 ${managerName} ${subjectName}`} description="先刷新环境扫描，或选择其他管理器。" />}</div> : null}
          {action === "uninstall" ? <label className="action-input">已安装{subjectName}<select aria-label={`待卸载${managerId === "homebrew" ? " Formula" : " pnpm 全局包"}`} value={uninstallTarget} onChange={(event) => setUninstallTarget(event.target.value)} disabled={props.isExecuting}><option value="">选择{subjectName}</option>{packages.map((pkg) => <option key={pkg.id} value={pkg.name}>{pkg.name} · {pkg.version}</option>)}</select><small>{managerId === "homebrew" ? "预检会检查已安装依赖方，Homebrew 仍可能拒绝不安全卸载。" : "只允许移除本次扫描识别到的 pnpm 全局包，并固定禁用 lifecycle scripts。"}</small></label> : null}
          {action === "cleanup" ? <div className="cleanup-description"><Icon name="packages" /><div><strong>预览 {managerName} 缓存清理</strong><p>{managerId === "homebrew" ? "先执行 brew cleanup --dry-run，确认后仅调用 brew cleanup。" : "展示共享 store 路径和当前扫描大小；确认后仅调用 pnpm store prune，不直接删除目录。"}</p></div></div> : null}
          <button className="button button--primary" onClick={() => void createPlan()} disabled={recoveryRequired || props.isPlanning || props.isExecuting || (action !== "cleanup" && targets.length === 0)}>{props.isPlanning ? "正在预检…" : "生成操作计划"}</button>
        </div>
      </section>

      {props.plan ? <section className="panel action-plan" aria-label="待确认操作计划">
        <div className="panel__header"><h2>待确认操作计划</h2><span className="action-status action-status--planned">待确认</span></div>
        <div className="action-plan__body">
          <div className="action-plan__summary"><div><span>管理器</span><strong>{managerLabels[props.plan.managerId]}</strong></div><div><span>操作</span><strong>{actionLabels[props.plan.action]}</strong></div><div><span>目标</span><strong>{props.plan.targets.join("、") || `${managerLabels[props.plan.managerId]} 缓存`}</strong></div><div><span>网络</span><strong>{props.plan.requiresNetwork ? "需要访问软件源" : "不主动联网"}</strong></div></div>
          <div><h3>固定命令预览</h3><code className="command-preview">{props.plan.commandPreview}</code></div>
          <div><h3>预检结果</h3><ul className="plan-lines">{props.plan.previewLines.map((line) => <li key={line}>{line}</li>)}</ul></div>
          <div><h3>风险提示</h3><ul className="plan-warnings">{props.plan.warnings.map((warning) => <li key={warning}><Icon name="warning" />{warning}</li>)}</ul></div>
          <label className="confirmation-field"><input type="checkbox" checked={confirmed} onChange={(event) => setConfirmed(event.target.checked)} /><span>我已核对管理器、目标、固定命令和风险，并确认修改本机环境。</span></label>
          <div className="action-plan__actions"><button className="button button--primary" onClick={() => { setConfirmed(false); void props.onExecute(); }} disabled={!confirmed || props.isExecuting}>确认并执行</button><button className="button button--secondary" onClick={props.onClearPlan} disabled={props.isExecuting}>放弃计划</button></div>
        </div>
      </section> : null}

      {props.isExecuting || props.progress.length ? <section className="panel action-progress-panel" aria-label="操作进度"><div className="panel__header"><h2>操作进度</h2>{props.isExecuting ? <span className="scan-indicator action-spinner"><Icon name="refresh" /></span> : null}</div><div className="action-progress-list">{props.progress.map((item, index) => <div key={`${item.timestamp}-${index}`}><time>{new Date(item.timestamp).toLocaleTimeString()}</time><span>{item.message}</span></div>)}</div>{props.isExecuting ? <div className="action-progress-panel__actions"><button className="button button--danger" onClick={() => void props.onCancel()} disabled={!canCancel}>终止操作</button><p>{canCancel ? "终止不会回滚已完成步骤；应用会强制重新扫描。" : "正在完成强制复扫，此阶段不能取消。"}</p></div> : null}</section> : null}
      {props.result ? <section className={`panel action-result action-result--${props.result.status}`} aria-label="操作结果"><div className="panel__header"><h2>操作结果</h2><span className={`action-status action-status--${props.result.status}`}>{statusLabels[props.result.status]}</span></div><div className="action-result__body"><p>{props.result.error ?? `${managerLabels[props.result.managerId]} 操作成功，环境扫描已更新。`}</p>{props.result.comparison ? <div className="result-changes"><strong>{props.result.comparison.changes.length} 项环境变化</strong>{props.result.comparison.changes.map((change) => <div key={`${change.entity}:${change.key}`}><span>{change.title}</span><small>{change.description}</small></div>)}</div> : <p>没有可比较的前后快照。</p>}</div></section> : null}

      <section className="panel action-audit" aria-label="操作审计记录">
        <div className="panel__header"><h2>最近操作审计</h2><div className="audit-filter"><select aria-label="审计管理器筛选" value={auditManager} onChange={(event) => setAuditManager(event.target.value as typeof auditManager)}><option value="all">全部管理器</option><option value="homebrew">Homebrew</option><option value="pnpm">pnpm</option></select><span className="count-label">{filteredAudit.length}</span></div></div>
        {filteredAudit.length ? <div className="audit-list">{filteredAudit.map((record) => <article key={record.actionId}><span className={`action-status action-status--${record.status}`}>{statusLabels[record.status]}</span><div><strong>{managerLabels[record.managerId]} · {actionLabels[record.action]} · {record.targets.join("、") || "缓存"}</strong><code>{record.commandPreview}</code>{record.status === "running" ? <small>需要重新扫描后确认实际状态</small> : null}</div><time>{new Date(record.finishedAt).toLocaleString()}</time></article>)}</div> : <EmptyState icon="history" title="没有匹配的写操作记录" description="完成受控操作后，结果会保存在本机审计记录中。" />}
      </section>
    </>
  );
}
