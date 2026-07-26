import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { apiErrorMessage } from "../lib/apiError";
import type {
  ActionCapability,
  EnvironmentScan,
  PackageAction,
  PackageActionAuditRecord,
  PackageActionPlan,
  PackageActionProgress,
  PackageActionResult,
  WritableManagerId,
} from "../types";

interface PackageActionState {
  plan?: PackageActionPlan;
  result?: PackageActionResult;
  audit: PackageActionAuditRecord[];
  capabilities: ActionCapability[];
  progress: PackageActionProgress[];
  isPlanning: boolean;
  isExecuting: boolean;
  isReconciling: boolean;
  error?: string;
}

const initialState: PackageActionState = {
  audit: [],
  capabilities: [],
  progress: [],
  isPlanning: false,
  isExecuting: false,
  isReconciling: false,
};

export function usePackageActions(onEnvironmentUpdated: (environment: EnvironmentScan) => void, scanVersion?: string) {
  const [state, setState] = useState(initialState);
  const activeActionId = useRef<string | undefined>(undefined);

  const loadAudit = useCallback(async () => {
    try {
      const [audit, capabilities] = await Promise.all([
        api.listPackageActionAudit(),
        api.getPackageActionCapabilities(),
      ]);
      setState((current) => ({ ...current, audit, capabilities }));
    } catch (error) {
      setState((current) => ({ ...current, error: messageFrom(error) }));
    }
  }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: 扫描完成（scanVersion 变化）后需重新拉取审计与能力
  useEffect(() => {
    void loadAudit();
  }, [loadAudit, scanVersion]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void api
      .listenToPackageActionProgress((progress) => {
        activeActionId.current = progress.actionId;
        setState((current) => ({ ...current, progress: [...current.progress.slice(-199), progress] }));
      })
      .then((cleanup) => {
        unlisten = cleanup;
      });
    return () => unlisten?.();
  }, []);

  const createPlan = useCallback(async (managerId: WritableManagerId, action: PackageAction, targets: string[]) => {
    setState((current) => ({
      ...current,
      plan: undefined,
      result: undefined,
      progress: [],
      isPlanning: true,
      error: undefined,
    }));
    try {
      const plan = await api.planPackageAction(managerId, action, targets);
      setState((current) => ({ ...current, plan, isPlanning: false }));
      return plan;
    } catch (error) {
      setState((current) => ({ ...current, isPlanning: false, error: messageFrom(error) }));
      return undefined;
    }
  }, []);

  const executePlan = useCallback(async () => {
    if (!state.plan) return;
    setState((current) => ({ ...current, isExecuting: true, result: undefined, progress: [], error: undefined }));
    try {
      const result = await api.executePackageAction(state.plan.id);
      if (result.environment) onEnvironmentUpdated(result.environment);
      const [audit, capabilities] = await Promise.all([
        api.listPackageActionAudit(),
        api.getPackageActionCapabilities(),
      ]);
      setState((current) => ({ ...current, plan: undefined, result, audit, capabilities, isExecuting: false }));
    } catch (error) {
      setState((current) => ({ ...current, isExecuting: false, error: messageFrom(error) }));
    } finally {
      activeActionId.current = undefined;
    }
  }, [onEnvironmentUpdated, state.plan]);

  const cancelAction = useCallback(async () => {
    if (activeActionId.current) await api.cancelPackageAction(activeActionId.current);
  }, []);

  const clearPlan = useCallback(() => {
    setState((current) => ({ ...current, plan: undefined, result: undefined, progress: [], error: undefined }));
  }, []);

  const reconcileAction = useCallback(
    async (actionId: string) => {
      setState((current) => ({ ...current, isReconciling: true, error: undefined }));
      try {
        const result = await api.reconcilePackageAction(actionId);
        onEnvironmentUpdated(result.environment);
        const [audit, capabilities] = await Promise.all([
          api.listPackageActionAudit(),
          api.getPackageActionCapabilities(),
        ]);
        setState((current) => ({ ...current, audit, capabilities, isReconciling: false }));
      } catch (error) {
        setState((current) => ({ ...current, isReconciling: false, error: messageFrom(error) }));
      }
    },
    [onEnvironmentUpdated],
  );

  return { ...state, createPlan, executePlan, cancelAction, clearPlan, reconcileAction, reloadAudit: loadAudit };
}

function messageFrom(error: unknown): string {
  return apiErrorMessage(error, "软件包操作失败，请查看审计记录。");
}
