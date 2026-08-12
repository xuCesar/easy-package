import { useCatalogSearch } from "../hooks/useCatalogSearch";
import { usePackageActions } from "../hooks/usePackageActions";
import type { UpgradePlanPrefill } from "../lib/upgradePlanBridge";
import type { EnvironmentScan, ManagedPackage, ScanSettings } from "../types";
import { ActionCenterPage } from "./ActionCenterPage";

interface ActionCenterContainerProps {
  packages: ManagedPackage[];
  scanSettings: ScanSettings;
  scannedAt?: string;
  upgradePrefill?: UpgradePlanPrefill;
  onApplyEnvironment: (environment: EnvironmentScan) => void;
  onUpdateSettings: (settings: ScanSettings) => Promise<void>;
  onRefresh: () => void;
}

// 写操作与目录搜索的 hooks 只在操作中心挂载；离开页面即卸载。
// 执行中途离开时，后端照常完成并留下 Running 审计，回来后走既有的重扫核对流程。
export function ActionCenterContainer({
  packages,
  scanSettings,
  scannedAt,
  upgradePrefill,
  onApplyEnvironment,
  onUpdateSettings,
  onRefresh,
}: ActionCenterContainerProps) {
  const packageActions = usePackageActions(onApplyEnvironment, scannedAt);
  const catalogSearch = useCatalogSearch();

  return (
    <ActionCenterPage
      upgradePrefill={upgradePrefill}
      packages={packages}
      scanSettings={scanSettings}
      capabilities={packageActions.capabilities}
      catalogResponse={catalogSearch.response}
      isCatalogSearching={catalogSearch.isSearching}
      catalogError={catalogSearch.error}
      plan={packageActions.plan}
      result={packageActions.result}
      audit={packageActions.audit}
      progress={packageActions.progress}
      isPlanning={packageActions.isPlanning}
      isExecuting={packageActions.isExecuting}
      isReconciling={packageActions.isReconciling}
      error={packageActions.error}
      onSearchCatalog={catalogSearch.search}
      onCancelCatalogSearch={catalogSearch.cancel}
      onClearCatalogSearch={catalogSearch.clear}
      onCreatePlan={packageActions.createPlan}
      onExecute={packageActions.executePlan}
      onCancel={packageActions.cancelAction}
      onReconcile={packageActions.reconcileAction}
      onClearPlan={packageActions.clearPlan}
      onUpdateSettings={onUpdateSettings}
      onRefresh={onRefresh}
    />
  );
}
