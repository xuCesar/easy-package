import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../lib/apiError";
import type { EnvironmentScan, ScanProgress } from "../types";
import { useDevPkg } from "./useDevPkg";

const { mocks } = vi.hoisted(() => ({
  mocks: {
    scanEnvironment: vi.fn<(scanId: string) => Promise<EnvironmentScan>>(),
    cancelEnvironmentScan: vi.fn<(scanId: string) => Promise<void>>(),
    listenToScanProgress: vi.fn<(listener: (progress: ScanProgress) => void) => Promise<() => void>>(),
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

  it("扫描进行中重复 refresh 被忽略，不会丢失进度与取消能力", async () => {
    const { result } = renderHook(() => useDevPkg());
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

  it("扫描失败展示错误，且不影响后续 refresh", async () => {
    const { result } = renderHook(() => useDevPkg());
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
});
