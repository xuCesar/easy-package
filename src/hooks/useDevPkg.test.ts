import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../lib/apiError";
import { mockScan } from "../mock-data";
import type { EnvironmentScan, ProjectAnalysis, ScanProgress, ScanSettings } from "../types";
import { useDevPkg } from "./useDevPkg";

const { mocks } = vi.hoisted(() => ({
  mocks: {
    getLatestSnapshot: vi.fn<() => Promise<EnvironmentScan | null>>(),
    scanEnvironment: vi.fn<(scanId: string) => Promise<EnvironmentScan>>(),
    cancelEnvironmentScan: vi.fn<(scanId: string) => Promise<void>>(),
    listenToScanProgress: vi.fn<(listener: (progress: ScanProgress) => void) => Promise<() => void>>(),
    updateScanSettings: vi.fn<(settings: ScanSettings) => Promise<ProjectAnalysis>>(),
  },
}));

vi.mock("../api", () => ({ api: mocks }));

interface Deferred<T> {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
}

const createDeferred = <T>(): Deferred<T> => {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
};

const fakeScan = {
  managers: [],
  packages: [],
  projects: [],
  healthIssues: [],
  scanRoots: [],
} as unknown as EnvironmentScan;

describe("useDevPkg 扫描竞态", () => {
  let scanCalls: Array<{ scanId: string; deferred: Deferred<EnvironmentScan> }>;
  let emitProgress: ((progress: ScanProgress) => void) | undefined;

  beforeEach(() => {
    vi.clearAllMocks();
    scanCalls = [];
    emitProgress = undefined;
    mocks.getLatestSnapshot.mockResolvedValue(null);
    mocks.scanEnvironment.mockImplementation((scanId) => {
      const deferred = createDeferred<EnvironmentScan>();
      scanCalls.push({ scanId, deferred });
      return deferred.promise;
    });
    mocks.cancelEnvironmentScan.mockResolvedValue(undefined);
    mocks.listenToScanProgress.mockImplementation(async (listener) => {
      emitProgress = listener;
      return () => {
        emitProgress = undefined;
      };
    });
  });

  afterEach(cleanup);

  it("启动时只读取最近快照，不自动扫描", async () => {
    mocks.getLatestSnapshot.mockResolvedValue(fakeScan);
    const { result } = renderHook(() => useDevPkg());

    await waitFor(() => expect(result.current.status).toBe("ready"));
    expect(result.current.data).toBe(fakeScan);
    expect(mocks.getLatestSnapshot).toHaveBeenCalledTimes(1);
    expect(mocks.scanEnvironment).not.toHaveBeenCalled();
  });

  it("没有快照时进入待扫描状态", async () => {
    const { result } = renderHook(() => useDevPkg());

    await waitFor(() => expect(result.current.status).toBe("idle"));
    expect(result.current.data).toBeUndefined();
    expect(mocks.scanEnvironment).not.toHaveBeenCalled();
  });

  it("扫描进行中重复 refresh 被忽略，不会丢失进度与取消能力", async () => {
    const { result } = renderHook(() => useDevPkg());
    await waitFor(() => expect(result.current.status).toBe("idle"));
    act(() => void result.current.refresh());
    await waitFor(() => expect(scanCalls).toHaveLength(1));
    const firstScanId = scanCalls[0].scanId;

    await act(async () => {
      await result.current.refresh();
    });
    expect(scanCalls).toHaveLength(1);

    const progress: ScanProgress = { scanId: firstScanId, phase: "managers", completed: 5, total: 13 };
    act(() => emitProgress?.(progress));
    expect(result.current.scanProgress).toEqual(progress);

    await act(async () => {
      await result.current.cancelScan();
    });
    expect(mocks.cancelEnvironmentScan).toHaveBeenCalledExactlyOnceWith(firstScanId);

    await act(async () => {
      scanCalls[0].deferred.reject(new ApiError({ code: "SCAN_CANCELLED", message: "扫描已取消" }));
      await scanCalls[0].deferred.promise.catch(() => undefined);
    });
    expect(result.current.notice).toBe("本次扫描已取消，保留上次成功结果。");
    expect(result.current.scanProgress).toBeUndefined();
  });

  it("扫描结束后可以再次 refresh，并写回新结果", async () => {
    const { result } = renderHook(() => useDevPkg());
    await waitFor(() => expect(result.current.status).toBe("idle"));
    act(() => void result.current.refresh());
    await waitFor(() => expect(scanCalls).toHaveLength(1));

    await act(async () => {
      scanCalls[0].deferred.resolve(fakeScan);
      await scanCalls[0].deferred.promise;
    });
    await waitFor(() => expect(result.current.data).toBe(fakeScan));
    expect(result.current.isLoading).toBe(false);

    await act(async () => {
      void result.current.refresh();
    });
    expect(scanCalls).toHaveLength(2);
    expect(scanCalls[1].scanId).not.toBe(scanCalls[0].scanId);
    expect(result.current.isLoading).toBe(true);

    const nextScan = { ...fakeScan };
    await act(async () => {
      scanCalls[1].deferred.resolve(nextScan);
      await scanCalls[1].deferred.promise;
    });
    await waitFor(() => expect(result.current.data).toBe(nextScan));
  });

  it("联网策略切换后立即使旧更新结果失效，等待重新扫描", async () => {
    const offlineScan = structuredClone(mockScan);
    const registrySettings = { ...offlineScan.scanSettings, networkPolicy: "registry" as const };
    mocks.getLatestSnapshot.mockResolvedValue(offlineScan);
    mocks.updateScanSettings.mockResolvedValue({
      projects: offlineScan.projects,
      dependencyInsights: offlineScan.dependencyInsights,
      workspaces: offlineScan.workspaces,
      runtimeAssessments: offlineScan.runtimeAssessments,
      healthIssues: offlineScan.healthIssues,
      scanSettings: registrySettings,
    });
    const { result } = renderHook(() => useDevPkg());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    await act(async () => {
      await result.current.updateScanSettings(registrySettings);
    });

    expect(result.current.data?.scanSettings.networkPolicy).toBe("registry");
    expect(result.current.data?.packages.find((pkg) => pkg.id === "homebrew:git")).toMatchObject({
      version: "2.49.0",
      latestVersion: undefined,
      updateStatus: "unknown",
    });
  });

  it("扫描失败展示错误，且不影响后续 refresh", async () => {
    const { result } = renderHook(() => useDevPkg());
    await waitFor(() => expect(result.current.status).toBe("idle"));
    act(() => void result.current.refresh());
    await waitFor(() => expect(scanCalls).toHaveLength(1));

    await act(async () => {
      scanCalls[0].deferred.reject(new Error("扫描超时"));
      await scanCalls[0].deferred.promise.catch(() => undefined);
    });
    expect(result.current.error).toBe("扫描超时");
    expect(result.current.scanProgress).toBeUndefined();

    await act(async () => {
      void result.current.refresh();
    });
    expect(scanCalls).toHaveLength(2);
  });

  it("手动扫描开始后忽略迟到的启动快照", async () => {
    const snapshot = createDeferred<EnvironmentScan | null>();
    mocks.getLatestSnapshot.mockReturnValue(snapshot.promise);
    const { result } = renderHook(() => useDevPkg());

    act(() => void result.current.refresh());
    await waitFor(() => expect(scanCalls).toHaveLength(1));

    const scannedData = { ...fakeScan };
    await act(async () => {
      snapshot.resolve(fakeScan);
      scanCalls[0].deferred.resolve(scannedData);
      await Promise.all([snapshot.promise, scanCalls[0].deferred.promise]);
    });

    await waitFor(() => expect(result.current.data).toBe(scannedData));
    expect(result.current.status).toBe("ready");
  });
});
