import { useMemo, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import { RegistryPolicyNotice } from "../components/RegistryPolicyNotice";
import {
  buildUpgradePlanPrefillsByManager,
  isWritableManagerId,
  type UpgradePlanPrefill,
} from "../lib/upgradePlanBridge";
import type {
  ActionCapability,
  CatalogSearchResponse,
  ManagedPackage,
  PackageAction,
  PackageActionAuditRecord,
  PackageActionPlan,
  PackageActionProgress,
  PackageActionResult,
  PackageManagerId,
  ScanSettings,
  WritableManagerId,
} from "../types";

interface ActionCenterPageProps {
  packages: ManagedPackage[];
  upgradePrefill?: UpgradePlanPrefill;
  scanSettings: ScanSettings;
  capabilities: ActionCapability[];
  catalogResponse?: CatalogSearchResponse;
  isCatalogSearching: boolean;
  catalogError?: string;
  plan?: PackageActionPlan;
  result?: PackageActionResult;
  audit: PackageActionAuditRecord[];
  progress: PackageActionProgress[];
  isPlanning: boolean;
  isExecuting: boolean;
  isReconciling: boolean;
  error?: string;
  onSearchCatalog: (managerId: WritableManagerId, query: string) => Promise<CatalogSearchResponse | undefined>;
  onCancelCatalogSearch: () => Promise<void>;
  onClearCatalogSearch: () => void;
  onCreatePlan: (
    managerId: WritableManagerId,
    action: PackageAction,
    targets: string[],
  ) => Promise<PackageActionPlan | undefined>;
  onExecute: () => Promise<void>;
  onCancel: () => Promise<void>;
  onReconcile: (actionId: string) => Promise<void>;
  onClearPlan: () => void;
  onUpdateSettings: (settings: ScanSettings) => Promise<void>;
  onRefresh: () => void;
}

const actionLabels: Record<PackageAction, string> = {
  install: "安装",
  upgrade: "升级",
  uninstall: "卸载",
  cleanup: "缓存清理",
};
const managerLabels: Record<WritableManagerId, string> = { homebrew: "Homebrew", npm: "npm", pnpm: "pnpm" };
// 计划/审计记录的 managerId 在 wire 上是完整 PackageManagerId；后端保证只会出现可写管理器，
// 这里展示层兜底为原始 id，不做静默断言。
const managerDisplay = (managerId: PackageManagerId): string =>
  isWritableManagerId(managerId) ? managerLabels[managerId] : managerId;
const statusLabels = { planned: "待确认", running: "可能中断", succeeded: "成功", failed: "失败", unknown: "状态未知" };
const outcomeLabels = { applied: "已观察到生效", notApplied: "未观察到变化", ambiguous: "结果仍不明确" };
const actionDisplay = (managerId: PackageManagerId, action: PackageAction) =>
  managerId === "npm" && action === "cleanup" ? "缓存校验与回收" : actionLabels[action];

export function ActionCenterPage(props: ActionCenterPageProps) {
  const [managerId, setManagerId] = useState<WritableManagerId>(props.upgradePrefill?.managerId ?? "homebrew");
  const [action, setAction] = useState<PackageAction>(props.upgradePrefill ? "upgrade" : "install");
  const [upgradePrefillNotice, setUpgradePrefillNotice] = useState(props.upgradePrefill);
  const [installTarget, setInstallTarget] = useState("");
  const [catalogQuery, setCatalogQuery] = useState("");
  const [upgradeTargets, setUpgradeTargets] = useState<string[]>(props.upgradePrefill?.targets ?? []);
  const [uninstallTarget, setUninstallTarget] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [auditManager, setAuditManager] = useState<"all" | WritableManagerId>("all");
  const [expandedAuditId, setExpandedAuditId] = useState<string>();
  const packages = useMemo(
    () =>
      [...props.packages.filter((pkg) => pkg.managerId === managerId)].sort((left, right) =>
        left.name.localeCompare(right.name),
      ),
    [managerId, props.packages],
  );
  const filteredAudit = useMemo(
    () => props.audit.filter((record) => auditManager === "all" || record.managerId === auditManager),
    [auditManager, props.audit],
  );
  const recoveryRecords = props.audit.filter((record) => record.status === "running" || record.rescanRequired);
  const recoveryRequired = recoveryRecords.length > 0;
  const pendingUpgradePrefills =
    props.scanSettings.networkPolicy === "registry" ? buildUpgradePlanPrefillsByManager(props.packages) : [];
  const pendingUpdateCount = pendingUpgradePrefills.reduce(
    (total, prefill) => total + prefill.targets.length + prefill.truncatedCount,
    0,
  );
  const capability = props.capabilities.find((item) => item.managerId === managerId && item.action === action);
  const canCancel = props.progress.at(-1)?.cancellable === true;
  const targets =
    action === "install"
      ? [installTarget.trim()].filter(Boolean)
      : action === "upgrade"
        ? upgradeTargets
        : action === "uninstall"
          ? [uninstallTarget].filter(Boolean)
          : [];
  const managerName = managerLabels[managerId];
  const subjectName = managerId === "homebrew" ? "Formula" : "全局包";

  const resetPlan = () => {
    setConfirmed(false);
    props.onClearPlan();
  };
  const changeManager = (next: WritableManagerId) => {
    setManagerId(next);
    setUpgradePrefillNotice(undefined);
    setInstallTarget("");
    setUpgradeTargets([]);
    setUninstallTarget("");
    setCatalogQuery("");
    props.onClearCatalogSearch();
    resetPlan();
  };
  const changeAction = (next: PackageAction) => {
    setAction(next);
    setUpgradePrefillNotice(undefined);
    if (next !== "install") props.onClearCatalogSearch();
    resetPlan();
  };
  const createPlan = async () => {
    setConfirmed(false);
    await props.onCreatePlan(managerId, action, targets);
  };
  const searchCatalog = () => void props.onSearchCatalog(managerId, catalogQuery);
  const selectCatalogResult = (name: string) => {
    setInstallTarget(name);
    resetPlan();
  };
  const selectPendingUpgrade = (prefill: UpgradePlanPrefill) => {
    setManagerId(prefill.managerId);
    setAction("upgrade");
    setUpgradePrefillNotice(undefined);
    setInstallTarget("");
    setUpgradeTargets(prefill.targets);
    setUninstallTarget("");
    setCatalogQuery("");
    props.onClearCatalogSearch();
    resetPlan();
  };

  return (
    <>
      <PageHeader
        title="操作中心"
        description="通过后端固定白名单执行 Homebrew Formula、npm 与 pnpm 全局包操作；每次修改都先预检、确认并在完成后重新扫描。"
      />
      <section className="panel pending-actions" aria-label="扫描建议与恢复">
        <div className="panel__header">
          <h2>扫描建议</h2>
          <span className="count-label">{recoveryRecords.length + pendingUpdateCount}</span>
        </div>
        <div className="pending-actions__list">
          {recoveryRequired ? (
            <article className="pending-action pending-action--recovery">
              <Icon name="warning" />
              <div>
                <strong>上次操作可能没做完</strong>
                <span>{recoveryRecords.length} 条操作需要先重新扫描并核对实际结果，再继续执行新的写操作。</span>
              </div>
              <button
                className="button button--primary"
                onClick={() => void props.onReconcile(recoveryRecords[0].actionId)}
                disabled={props.isReconciling}
              >
                {props.isReconciling ? "正在核对…" : "重新扫描并核对"}
              </button>
            </article>
          ) : null}
          {props.scanSettings.networkPolicy === "offline" ? (
            <article className="pending-action pending-action--quiet">
              <Icon name="info" />
              <div>
                <strong>未检查更新</strong>
                <span>当前为离线模式，不会把已有版本信息当作可更新任务。</span>
              </div>
            </article>
          ) : (
            pendingUpgradePrefills.map((prefill) => {
              const updateCount = prefill.targets.length + prefill.truncatedCount;
              return (
                <button
                  key={prefill.managerId}
                  type="button"
                  className="pending-action pending-action--upgrade"
                  aria-label={`处理 ${managerLabels[prefill.managerId]} 的 ${updateCount} 个可更新项`}
                  onClick={() => selectPendingUpgrade(prefill)}
                  disabled={props.isExecuting}
                >
                  <Icon name="refresh" />
                  <span>
                    <strong>
                      {managerLabels[prefill.managerId]} · {updateCount} 个可更新项
                    </strong>
                    <small>
                      {prefill.targets.join("、")}
                      {prefill.truncatedCount > 0 ? ` 等，单次先处理 ${prefill.targets.length} 个` : ""}
                    </small>
                  </span>
                  <span className="pending-action__cta">
                    预填升级 <Icon name="chevron" />
                  </span>
                </button>
              );
            })
          )}
          {!recoveryRequired &&
          props.scanSettings.networkPolicy === "registry" &&
          pendingUpgradePrefills.length === 0 ? (
            <p className="pending-actions__empty">没有待处理的操作</p>
          ) : null}
        </div>
      </section>
      <div className="write-boundary-banner">
        <Icon name="warning" />
        <div>
          <strong>受控写入模式</strong>
          <span>不使用 Shell、不接受自定义参数、不请求 sudo；npm 与 pnpm 固定禁用 lifecycle scripts。</span>
        </div>
      </div>
      {action === "install" ? (
        <RegistryPolicyNotice
          scanSettings={props.scanSettings}
          onUpdateSettings={props.onUpdateSettings}
          onRefresh={props.onRefresh}
        />
      ) : null}
      {props.error ? (
        <div className="inline-alert inline-alert--error">
          <Icon name="warning" />
          <span>{props.error}</span>
        </div>
      ) : null}
      {upgradePrefillNotice ? (
        <div className="inline-alert inline-alert--selection" role="status">
          <Icon name="info" />
          <span>
            {`当前计划已从软件包页带入 ${upgradePrefillNotice.targets.length} 个 ${managerLabels[upgradePrefillNotice.managerId]} 升级目标。`}
            {upgradePrefillNotice.truncatedCount > 0
              ? `超出单次计划上限，已截断 ${upgradePrefillNotice.truncatedCount} 个，可在执行后分批处理。`
              : ""}
            {upgradePrefillNotice.otherWritableCount > 0
              ? `另有 ${upgradePrefillNotice.otherWritableCount} 个可更新包属于其他可写管理器，请切换管理器后单独生成计划。`
              : ""}
            {upgradePrefillNotice.unwritableCount > 0
              ? `${upgradePrefillNotice.unwritableCount} 个可更新包不属于受控可写管理器，已被过滤。`
              : ""}
          </span>
        </div>
      ) : null}

      <div className="action-controls" role="group" aria-label="操作类型选择">
        <div className="action-control-group">
          <span className="action-control-group__label">包管理器</span>
          <div className="view-tabs action-manager-tabs" role="tablist" aria-label="写操作包管理器">
            {(["homebrew", "npm", "pnpm"] as const).map((item) => (
              <button
                key={item}
                role="tab"
                aria-selected={managerId === item}
                className={managerId === item ? "view-tab view-tab--active" : "view-tab"}
                onClick={() => changeManager(item)}
                disabled={props.isExecuting}
              >
                {managerLabels[item]}
              </button>
            ))}
          </div>
        </div>
        <div className="action-control-group">
          <span className="action-control-group__label">操作类型</span>
          <div className="view-tabs action-tabs" role="tablist" aria-label="软件包操作">
            {(Object.keys(actionLabels) as PackageAction[]).map((item) => (
              <button
                key={item}
                role="tab"
                aria-selected={action === item}
                className={action === item ? "view-tab view-tab--active" : "view-tab"}
                onClick={() => changeAction(item)}
                disabled={props.isExecuting}
              >
                {actionDisplay(managerId, item)}
              </button>
            ))}
          </div>
        </div>
      </div>
      {capability ? (
        <section
          className={`panel action-capability action-capability--${capability.ready ? "ready" : "blocked"}`}
          aria-label="动作可用性"
        >
          <div className="panel__header">
            <h2>{capability.ready ? "可以生成计划" : "当前操作被阻止"}</h2>
            <span className={`action-status action-status--${capability.ready ? "succeeded" : "failed"}`}>
              {capability.ready ? "就绪" : "阻止"}
            </span>
          </div>
          <div className="capability-checks">
            {capability.checks.map((check) => (
              <div
                key={`${check.code}:${check.title}`}
                className={`capability-check capability-check--${check.status}`}
              >
                <strong>{check.title}</strong>
                <span>{check.detail}</span>
                <code>{check.code}</code>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      <section className="panel action-builder" aria-label="操作预检设置">
        <div className="panel__header">
          <h2>
            {actionDisplay(managerId, action)} {managerName} {subjectName}
          </h2>
          <span className={`manager-chip manager-chip--${managerId}`}>{managerName}</span>
        </div>
        <div className="action-builder__body">
          {action === "install" ? (
            <>
              <label className="action-input">
                {subjectName} 名称
                <input
                  aria-label={`待安装${managerId === "homebrew" ? " Formula" : ` ${managerName} 全局包`}`}
                  value={installTarget}
                  onChange={(event) => setInstallTarget(event.target.value)}
                  placeholder={managerId === "homebrew" ? "例如 jq 或 user/tap/formula" : "例如 eslint 或 @scope/tool"}
                  disabled={props.isExecuting}
                />
                <small>
                  {managerId === "homebrew"
                    ? "名称会经过严格语法校验，存在性由 Homebrew 执行时验证。"
                    : "仅接受普通包名或 @scope/name；拒绝版本、tag、URL、Git、workspace 和本地路径来源。"}
                </small>
              </label>
              <section className="catalog-search" aria-label="软件包目录搜索">
                <div>
                  <strong>从 {managerName} 目录搜索</strong>
                  <small>
                    {props.scanSettings.networkPolicy === "registry"
                      ? "仅使用固定只读搜索命令；搜索结果只会填入安装目标。"
                      : "当前为离线模式；允许检查更新并重新扫描后才能搜索目录。"}
                  </small>
                </div>
                <div className="catalog-search__controls">
                  <input
                    aria-label={`${managerName} 目录搜索词`}
                    value={catalogQuery}
                    onChange={(event) => setCatalogQuery(event.target.value)}
                    placeholder="至少输入 2 个字符"
                    disabled={
                      props.isExecuting || props.isCatalogSearching || props.scanSettings.networkPolicy === "offline"
                    }
                  />
                  <button
                    className="button button--secondary"
                    onClick={searchCatalog}
                    disabled={
                      props.isExecuting ||
                      props.isCatalogSearching ||
                      props.scanSettings.networkPolicy === "offline" ||
                      catalogQuery.trim().length < 2
                    }
                  >
                    {props.isCatalogSearching ? "正在搜索…" : "搜索目录"}
                  </button>
                  {props.isCatalogSearching ? (
                    <button className="button button--secondary" onClick={() => void props.onCancelCatalogSearch()}>
                      取消搜索
                    </button>
                  ) : null}
                </div>
                {props.catalogError ? (
                  <p className="catalog-search__message catalog-search__message--error">{props.catalogError}</p>
                ) : null}
                {props.catalogResponse ? (
                  <div className="catalog-search__results">
                    <p
                      className={
                        props.catalogResponse.status === "ready"
                          ? "catalog-search__message"
                          : "catalog-search__message catalog-search__message--error"
                      }
                    >
                      {props.catalogResponse.message}
                      {props.catalogResponse.blockerCode ? <code>{props.catalogResponse.blockerCode}</code> : null}
                    </p>
                    {props.catalogResponse.status === "ready" && props.catalogResponse.results.length === 0 ? (
                      <p className="catalog-search__empty">没有安全且可用于安装的匹配结果。</p>
                    ) : null}
                    {props.catalogResponse.results.map((result) => (
                      <article key={`${result.managerId}:${result.name}`}>
                        <div>
                          <strong>{result.name}</strong>
                          <span>
                            {result.version ? `目录版本 ${result.version}` : "目录未提供版本"}
                            {result.installed ? ` · 已安装 ${result.installedVersion ?? ""}` : ""}
                          </span>
                          {result.description ? <small>{result.description}</small> : null}
                        </div>
                        <button
                          className="button button--secondary"
                          onClick={() => selectCatalogResult(result.name)}
                          disabled={props.isExecuting}
                        >
                          {result.name === installTarget ? "已选中" : "用作安装目标"}
                        </button>
                      </article>
                    ))}
                  </div>
                ) : null}
              </section>
            </>
          ) : null}
          {action === "upgrade" ? (
            <div className="formula-selection">
              <div>
                <strong>选择待升级{subjectName}</strong>
                <small>最多选择 20 个当前扫描到的已安装包。</small>
              </div>
              {packages.length ? (
                <div className="formula-checks">
                  {packages.map((pkg) => (
                    <label key={pkg.id}>
                      <input
                        type="checkbox"
                        checked={upgradeTargets.includes(pkg.name)}
                        onChange={(event) =>
                          setUpgradeTargets((current) =>
                            event.target.checked ? [...current, pkg.name] : current.filter((name) => name !== pkg.name),
                          )
                        }
                        disabled={props.isExecuting}
                      />
                      <span>
                        <strong>{pkg.name}</strong>
                        <code>
                          {pkg.latestVersion
                            ? `${pkg.version} → ${pkg.latestVersion}`
                            : `${pkg.version} · 更新状态未知`}
                        </code>
                      </span>
                    </label>
                  ))}
                </div>
              ) : (
                <EmptyState
                  icon="packages"
                  title={`没有已安装的 ${managerName} ${subjectName}`}
                  description="先刷新环境扫描，或选择其他管理器。"
                />
              )}
            </div>
          ) : null}
          {action === "uninstall" ? (
            <label className="action-input">
              已安装{subjectName}
              <select
                aria-label={`待卸载${managerId === "homebrew" ? " Formula" : ` ${managerName} 全局包`}`}
                value={uninstallTarget}
                onChange={(event) => setUninstallTarget(event.target.value)}
                disabled={props.isExecuting}
              >
                <option value="">选择{subjectName}</option>
                {packages.map((pkg) => (
                  <option key={pkg.id} value={pkg.name}>
                    {pkg.name} · {pkg.version}
                  </option>
                ))}
              </select>
              <small>
                {managerId === "homebrew"
                  ? "预检会检查已安装依赖方，Homebrew 仍可能拒绝不安全卸载。"
                  : `只允许移除本次扫描识别到的 ${managerName} 全局包，并固定禁用 lifecycle scripts。`}
              </small>
            </label>
          ) : null}
          {action === "cleanup" ? (
            <div className="cleanup-description">
              <Icon name="packages" />
              <div>
                <strong>{managerId === "npm" ? "缓存校验与回收" : `预览 ${managerName} 缓存清理`}</strong>
                <p>
                  {managerId === "homebrew"
                    ? "先执行 brew cleanup --dry-run，确认后仅调用 brew cleanup。"
                    : managerId === "pnpm"
                      ? "展示共享 store 路径和当前扫描大小；确认后仅调用 pnpm store prune，不直接删除目录。"
                      : "读取 Node、global prefix 与 cache 路径后，仅调用 npm cache verify；不会执行 npm cache clean --force。"}
                </p>
              </div>
            </div>
          ) : null}
          <button
            className="button button--primary"
            onClick={() => void createPlan()}
            disabled={
              capability?.ready !== true ||
              recoveryRequired ||
              props.isPlanning ||
              props.isExecuting ||
              (action !== "cleanup" && targets.length === 0)
            }
          >
            {props.isPlanning ? "正在预检…" : "生成操作计划"}
          </button>
        </div>
      </section>

      {props.plan ? (
        <section className="panel action-plan" aria-label="待确认操作计划">
          <div className="panel__header">
            <h2>待确认操作计划</h2>
            <span className="action-status action-status--planned">待确认</span>
          </div>
          <div className="action-plan__body">
            <div className="action-plan__summary">
              <div>
                <span>管理器</span>
                <strong>{managerDisplay(props.plan.managerId)}</strong>
              </div>
              <div>
                <span>操作</span>
                <strong>{actionDisplay(props.plan.managerId, props.plan.action)}</strong>
              </div>
              <div>
                <span>目标</span>
                <strong>{props.plan.targets.join("、") || `${managerDisplay(props.plan.managerId)} 缓存`}</strong>
              </div>
              <div>
                <span>网络</span>
                <strong>{props.plan.requiresNetwork ? "需要访问软件源" : "不主动联网"}</strong>
              </div>
            </div>
            <div>
              <h3>固定命令预览</h3>
              <code className="command-preview">{props.plan.commandPreview}</code>
            </div>
            <div>
              <h3>预检结果</h3>
              <ul className="plan-lines">
                {props.plan.previewLines.map((line) => (
                  <li key={line}>{line}</li>
                ))}
              </ul>
            </div>
            <div>
              <h3>安全检查</h3>
              <ul className="plan-lines">
                {props.plan.checks.map((check) => (
                  <li key={`${check.code}:${check.title}`}>
                    {check.title}：{check.detail}
                  </li>
                ))}
              </ul>
            </div>
            <div>
              <h3>风险提示</h3>
              <ul className="plan-warnings">
                {props.plan.warnings.map((warning) => (
                  <li key={warning}>
                    <Icon name="warning" />
                    {warning}
                  </li>
                ))}
              </ul>
            </div>
            <label className="confirmation-field">
              <input type="checkbox" checked={confirmed} onChange={(event) => setConfirmed(event.target.checked)} />
              <span>我已核对管理器、目标、固定命令和风险，并确认修改本机环境。</span>
            </label>
            <div className="action-plan__actions">
              <button
                className="button button--primary"
                onClick={() => {
                  setConfirmed(false);
                  void props.onExecute();
                }}
                disabled={!confirmed || props.isExecuting}
              >
                确认并执行
              </button>
              <button className="button button--secondary" onClick={props.onClearPlan} disabled={props.isExecuting}>
                放弃计划
              </button>
            </div>
          </div>
        </section>
      ) : null}

      {props.isExecuting || props.progress.length ? (
        <section className="panel action-progress-panel" aria-label="操作进度">
          <div className="panel__header">
            <h2>操作进度</h2>
            {props.isExecuting ? (
              <span className="scan-indicator action-spinner">
                <Icon name="refresh" />
              </span>
            ) : null}
          </div>
          <div className="action-progress-list">
            {props.progress.map((item, index) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: 进度行只追加不重排，时间戳可能重复
              <div key={`${item.timestamp}-${index}`}>
                <time>{new Date(item.timestamp).toLocaleTimeString()}</time>
                <span>{item.message}</span>
              </div>
            ))}
          </div>
          {props.isExecuting ? (
            <div className="action-progress-panel__actions">
              <button className="button button--danger" onClick={() => void props.onCancel()} disabled={!canCancel}>
                终止操作
              </button>
              <p>{canCancel ? "终止不会回滚已完成步骤；应用会强制重新扫描。" : "正在完成强制复扫，此阶段不能取消。"}</p>
            </div>
          ) : null}
        </section>
      ) : null}
      {props.result ? (
        <section className={`panel action-result action-result--${props.result.status}`} aria-label="操作结果">
          <div className="panel__header">
            <h2>操作结果</h2>
            <span className={`action-status action-status--${props.result.status}`}>
              {statusLabels[props.result.status]}
            </span>
          </div>
          <div className="action-result__body">
            <p>{props.result.error ?? `${managerDisplay(props.result.managerId)} 操作成功，环境扫描已更新。`}</p>
            {props.result.comparison ? (
              <div className="result-changes">
                <strong>{props.result.comparison.changes.length} 项环境变化</strong>
                {props.result.comparison.changes.map((change) => (
                  <div key={`${change.entity}:${change.key}`}>
                    <span>{change.title}</span>
                    <small>{change.description}</small>
                  </div>
                ))}
              </div>
            ) : (
              <p>没有可比较的前后快照。</p>
            )}
          </div>
        </section>
      ) : null}

      <section className="panel action-audit" aria-label="操作审计记录">
        <div className="panel__header">
          <h2>最近操作审计</h2>
          <div className="audit-filter">
            <select
              aria-label="审计管理器筛选"
              value={auditManager}
              onChange={(event) => setAuditManager(event.target.value as typeof auditManager)}
            >
              <option value="all">全部管理器</option>
              <option value="homebrew">Homebrew</option>
              <option value="npm">npm</option>
              <option value="pnpm">pnpm</option>
            </select>
            <span className="count-label">{filteredAudit.length}</span>
          </div>
        </div>
        {filteredAudit.length ? (
          <div className="audit-list">
            {filteredAudit.map((record) => (
              <article key={record.actionId}>
                <span className={`action-status action-status--${record.status}`}>{statusLabels[record.status]}</span>
                <div>
                  <strong>
                    {managerDisplay(record.managerId)} · {actionDisplay(record.managerId, record.action)} ·{" "}
                    {record.targets.join("、") || "缓存"}
                  </strong>
                  <code>{record.commandPreview}</code>
                  {record.observedOutcome ? <small>{outcomeLabels[record.observedOutcome]}</small> : null}
                  {record.status === "running" || record.rescanRequired ? (
                    <small>需要重新扫描后确认实际状态</small>
                  ) : null}
                  {expandedAuditId === record.actionId ? (
                    <div className="audit-detail">
                      <p>{record.error ?? "命令没有报告错误。"}</p>
                      {record.evidence.map((item) => (
                        <span key={item}>{item}</span>
                      ))}
                      {record.logs.map((line, index) => (
                        // biome-ignore lint/suspicious/noArrayIndexKey: 日志行只读展示，内容可能重复
                        <code key={`${index}:${line}`}>{line}</code>
                      ))}
                    </div>
                  ) : null}
                </div>
                <div className="audit-actions">
                  <time>{new Date(record.finishedAt).toLocaleString()}</time>
                  <button
                    className="button button--secondary"
                    onClick={() =>
                      setExpandedAuditId((current) => (current === record.actionId ? undefined : record.actionId))
                    }
                  >
                    {expandedAuditId === record.actionId ? "收起详情" : "查看详情"}
                  </button>
                  {record.status === "running" || record.rescanRequired ? (
                    <button
                      className="button button--primary"
                      onClick={() => void props.onReconcile(record.actionId)}
                      disabled={props.isReconciling}
                    >
                      重新核对
                    </button>
                  ) : null}
                </div>
              </article>
            ))}
          </div>
        ) : (
          <EmptyState
            icon="history"
            title="没有匹配的写操作记录"
            description="完成受控操作后，结果会保存在本机审计记录中。"
          />
        )}
      </section>
    </>
  );
}
