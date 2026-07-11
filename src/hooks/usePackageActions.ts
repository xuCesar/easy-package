import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { EnvironmentScan, PackageAction, PackageActionAuditRecord, PackageActionPlan, PackageActionProgress, PackageActionResult, WritableManagerId } from "../types";

interface PackageActionState {
  plan?: PackageActionPlan;
  result?: PackageActionResult;
  audit: PackageActionAuditRecord[];
  progress: PackageActionProgress[];
  isPlanning: boolean;
  isExecuting: boolean;
  error?: string;
}

const initialState: PackageActionState = {
  audit: [],
  progress: [],
  isPlanning: false,
  isExecuting: false,
};

export function usePackageActions(onEnvironmentUpdated: (environment: EnvironmentScan) => void, scanVersion?: string) {
  const [state, setState] = useState(initialState);
  const activeActionId = useRef<string | undefined>(undefined);

  const loadAudit = useCallback(async () => {
    try {
      const audit = await api.listPackageActionAudit();
      setState((current) => ({ ...current, audit }));
    } catch (error) {
      setState((current) => ({ ...current, error: messageFrom(error) }));
    }
  }, []);

  useEffect(() => {
    void loadAudit();
  }, [loadAudit, scanVersion]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void api.listenToPackageActionProgress((progress) => {
      activeActionId.current = progress.actionId;
      setState((current) => ({ ...current, progress: [...current.progress.slice(-199), progress] }));
    }).then((cleanup) => {
      unlisten = cleanup;
    });
    return () => unlisten?.();
  }, []);

  const createPlan = useCallback(async (managerId: WritableManagerId, action: PackageAction, targets: string[]) => {
    setState((current) => ({ ...current, plan: undefined, result: undefined, progress: [], isPlanning: true, error: undefined }));
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
      const audit = await api.listPackageActionAudit();
      setState((current) => ({ ...current, plan: undefined, result, audit, isExecuting: false }));
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

  return { ...state, createPlan, executePlan, cancelAction, clearPlan, reloadAudit: loadAudit };
}

function messageFrom(error: unknown): string {
  if (error instanceof Error) return error.message;
  return typeof error === "string" ? error : "软件包操作失败，请查看审计记录。";
}
