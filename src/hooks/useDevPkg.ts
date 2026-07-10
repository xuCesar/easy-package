import { startTransition, useCallback, useEffect, useState } from "react";
import { api } from "../api";
import type { EnvironmentScan } from "../types";

interface DevPkgState {
  data?: EnvironmentScan;
  isLoading: boolean;
  error?: string;
}

const getErrorMessage = (error: unknown): string => {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "扫描失败，请查看诊断日志后重试。";
};

export const useDevPkg = () => {
  const [state, setState] = useState<DevPkgState>({ isLoading: true });

  const refresh = useCallback(async () => {
    setState((current) => ({ ...current, isLoading: true, error: undefined }));
    try {
      const data = await api.scanEnvironment();
      startTransition(() => setState({ data, isLoading: false }));
    } catch (error) {
      setState((current) => ({ ...current, isLoading: false, error: getErrorMessage(error) }));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const addRoot = useCallback(async (path: string) => {
    const projects = await api.addScanRoot(path);
    setState((current) => current.data ? { ...current, data: { ...current.data, projects, scanRoots: current.data.scanRoots.includes(path) ? current.data.scanRoots : [...current.data.scanRoots, path] } } : current);
  }, []);

  const removeRoot = useCallback(async (path: string) => {
    const projects = await api.removeScanRoot(path);
    setState((current) => current.data ? { ...current, data: { ...current.data, projects, scanRoots: current.data.scanRoots.filter((root) => root !== path) } } : current);
  }, []);

  return { ...state, refresh, addRoot, removeRoot };
};
