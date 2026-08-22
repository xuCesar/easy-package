import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { apiErrorCode, apiErrorMessage } from "../lib/apiError";
import type { EnvironmentScan, ReportFormat, ScanProgress, ScanSettings } from "../types";

interface DevPkgState {
  data?: EnvironmentScan;
  error?: string;
  status: "initializing" | "idle" | "scanning" | "ready" | "error";
}

const invalidatePackageUpdateResults = (packages: EnvironmentScan["packages"]): EnvironmentScan["packages"] =>
  packages.map((pkg) => ({ ...pkg, latestVersion: undefined, updateStatus: "unknown" }));

export const useDevPkg = () => {
  const [state, setState] = useState<DevPkgState>({ status: "initializing" });
  const [scanProgress, setScanProgress] = useState<ScanProgress>();
  const [notice, setNotice] = useState<string>();
  const activeScanId = useRef<string | undefined>(undefined);
  const startupLoadGeneration = useRef(0);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void api
      .listenToScanProgress((progress) => {
        if (progress.scanId === activeScanId.current) setScanProgress(progress);
      })
      .then((cleanup) => {
        if (disposed) cleanup();
        else unlisten = cleanup;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    const generation = ++startupLoadGeneration.current;
    void api
      .getLatestSnapshot()
      .then((data) => {
        if (startupLoadGeneration.current !== generation) return;
        setState({ data: data ?? undefined, status: data ? "ready" : "idle" });
      })
      .catch((error) => {
        if (startupLoadGeneration.current !== generation) return;
        setState({
          error: apiErrorMessage(error, "无法读取上次扫描记录，仍可重新扫描。"),
          status: "error",
        });
      });
    return () => {
      if (startupLoadGeneration.current === generation) startupLoadGeneration.current += 1;
    };
  }, []);

  const refresh = useCallback(async () => {
    // 后端拒绝并发扫描；若已有扫描进行中则忽略本次调用，避免覆盖 activeScanId 导致进度与取消失效
    if (activeScanId.current) return;
    startupLoadGeneration.current += 1;
    const scanId = crypto.randomUUID();
    activeScanId.current = scanId;
    setState((current) => ({ ...current, error: undefined, status: "scanning" }));
    setScanProgress({ scanId, phase: "managers", completed: 0, total: 13 });
    setNotice(undefined);
    try {
      const data = await api.scanEnvironment(scanId);
      if (activeScanId.current !== scanId) return;
      setState({ data, status: "ready" });
    } catch (error) {
      if (activeScanId.current !== scanId) return;
      if (apiErrorCode(error) === "SCAN_CANCELLED") {
        setState((current) => ({ ...current, error: undefined, status: current.data ? "ready" : "idle" }));
        setNotice("本次扫描已取消，保留上次成功结果。");
      } else {
        setState((current) => ({
          ...current,
          error: apiErrorMessage(error, "扫描失败，请查看诊断日志后重试。"),
          status: "error",
        }));
      }
    } finally {
      if (activeScanId.current === scanId) {
        activeScanId.current = undefined;
        setScanProgress(undefined);
      }
    }
  }, []);

  const addRoot = useCallback(async (path: string) => {
    const analysis = await api.addScanRoot(path);
    setState((current) =>
      current.data
        ? {
            ...current,
            data: {
              ...current.data,
              ...analysis,
              scanRoots: current.data.scanRoots.includes(path)
                ? current.data.scanRoots
                : [...current.data.scanRoots, path],
            },
          }
        : current,
    );
  }, []);

  const removeRoot = useCallback(async (path: string) => {
    const analysis = await api.removeScanRoot(path);
    setState((current) =>
      current.data
        ? {
            ...current,
            data: { ...current.data, ...analysis, scanRoots: current.data.scanRoots.filter((root) => root !== path) },
          }
        : current,
    );
  }, []);

  const updateScanSettings = useCallback(async (settings: ScanSettings) => {
    const analysis = await api.updateScanSettings(settings);
    setState((current) => {
      if (!current.data) return current;
      const networkPolicyChanged = current.data.scanSettings.networkPolicy !== analysis.scanSettings.networkPolicy;
      return {
        ...current,
        data: {
          ...current.data,
          ...analysis,
          packages: networkPolicyChanged
            ? invalidatePackageUpdateResults(current.data.packages)
            : current.data.packages,
        },
      };
    });
  }, []);

  const exportEnvironmentReport = useCallback((format: ReportFormat) => api.exportEnvironmentReport(format), []);
  const getProjectDependencyGraph = useCallback(
    (projectPath: string) => api.getProjectDependencyGraph(projectPath),
    [],
  );
  const getProjectSupplyChainReport = useCallback(
    (projectPath: string) => api.getProjectSupplyChainReport(projectPath),
    [],
  );
  const exportProjectSbom = useCallback((projectPath: string) => api.exportProjectSbom(projectPath), []);
  const applyEnvironment = useCallback((data: EnvironmentScan) => {
    setState({ data, status: "ready" });
  }, []);

  const cancelScan = useCallback(async () => {
    if (activeScanId.current) await api.cancelEnvironmentScan(activeScanId.current);
  }, []);

  return {
    ...state,
    isInitializing: state.status === "initializing",
    isLoading: state.status === "scanning",
    scanProgress,
    notice,
    refresh,
    cancelScan,
    addRoot,
    removeRoot,
    updateScanSettings,
    exportEnvironmentReport,
    getProjectDependencyGraph,
    getProjectSupplyChainReport,
    exportProjectSbom,
    applyEnvironment,
  };
};
