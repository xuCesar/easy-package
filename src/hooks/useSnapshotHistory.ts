import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import type { ReportFormat, SnapshotComparison, SnapshotSummary } from "../types";

interface SnapshotHistoryState {
  summaries: SnapshotSummary[];
  comparison?: SnapshotComparison;
  isLoading: boolean;
  error?: string;
}

const errorMessage = (error: unknown) => error instanceof Error ? error.message : String(error);

export function useSnapshotHistory(scannedAt?: string) {
  const [state, setState] = useState<SnapshotHistoryState>({ summaries: [], isLoading: false });

  const load = useCallback(async () => {
    setState((current) => ({ ...current, isLoading: true, error: undefined }));
    try {
      const summaries = await api.listSnapshotSummaries();
      const comparison = summaries.length > 1
        ? await api.compareSnapshots(summaries[1].id, summaries[0].id)
        : undefined;
      setState({ summaries, comparison, isLoading: false });
    } catch (error) {
      setState((current) => ({ ...current, isLoading: false, error: errorMessage(error) }));
    }
  }, []);

  useEffect(() => {
    if (scannedAt) void load();
  }, [load, scannedAt]);

  const compare = useCallback(async (baselineId: number, currentId: number) => {
    const comparison = await api.compareSnapshots(baselineId, currentId);
    setState((state) => ({ ...state, comparison }));
    return comparison;
  }, []);

  const exportComparison = useCallback(
    (format: ReportFormat, baselineId: number, currentId: number) => api.exportSnapshotComparisonReport(format, baselineId, currentId),
    [],
  );

  return { ...state, load, compare, exportComparison };
}
