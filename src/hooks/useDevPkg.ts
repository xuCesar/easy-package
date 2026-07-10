import { startTransition, useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { EnvironmentScan, ScanProgress } from "../types";

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
  const [scanProgress, setScanProgress] = useState<ScanProgress>();
  const [notice, setNotice] = useState<string>();
  const activeScanId = useRef<string | undefined>(undefined);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void api.listenToScanProgress((progress) => {
      if (progress.scanId === activeScanId.current) setScanProgress(progress);
    }).then((cleanup) => {
      unlisten = cleanup;
    });
    return () => unlisten?.();
  }, []);

  const refresh = useCallback(async () => {
    const scanId = crypto.randomUUID();
    activeScanId.current = scanId;
    setState((current) => ({ ...current, isLoading: true, error: undefined }));
    setScanProgress({ scanId, phase: "managers", completed: 0, total: 10 });
    setNotice(undefined);
    try {
      const data = await api.scanEnvironment(scanId);
      startTransition(() => setState({ data, isLoading: false }));
    } catch (error) {
      if (getErrorMessage(error) === "扫描已取消") {
        setState((current) => ({ ...current, isLoading: false, error: undefined }));
        setNotice("本次扫描已取消，保留上次成功结果。");
      } else {
        setState((current) => ({ ...current, isLoading: false, error: getErrorMessage(error) }));
      }
    } finally {
      if (activeScanId.current === scanId) {
        activeScanId.current = undefined;
        setScanProgress(undefined);
      }
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const addRoot = useCallback(async (path: string) => {
    const analysis = await api.addScanRoot(path);
    setState((current) => current.data ? { ...current, data: { ...current.data, ...analysis, scanRoots: current.data.scanRoots.includes(path) ? current.data.scanRoots : [...current.data.scanRoots, path] } } : current);
  }, []);

  const removeRoot = useCallback(async (path: string) => {
    const analysis = await api.removeScanRoot(path);
    setState((current) => current.data ? { ...current, data: { ...current.data, ...analysis, scanRoots: current.data.scanRoots.filter((root) => root !== path) } } : current);
  }, []);

  const cancelScan = useCallback(async () => {
    if (activeScanId.current) await api.cancelEnvironmentScan(activeScanId.current);
  }, []);

  return { ...state, scanProgress, notice, refresh, cancelScan, addRoot, removeRoot };
};
