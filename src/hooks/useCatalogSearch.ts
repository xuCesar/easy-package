import { useCallback, useRef, useState } from "react";
import { api } from "../api";
import { apiErrorMessage } from "../lib/apiError";
import type { CatalogSearchResponse, WritableManagerId } from "../types";

interface CatalogSearchState {
  response?: CatalogSearchResponse;
  isSearching: boolean;
  error?: string;
}

export function useCatalogSearch() {
  const [state, setState] = useState<CatalogSearchState>({ isSearching: false });
  const activeSearchId = useRef<string | undefined>(undefined);

  const search = useCallback(async (managerId: WritableManagerId, query: string) => {
    if (activeSearchId.current) await api.cancelPackageCatalogSearch(activeSearchId.current);
    const searchId = crypto.randomUUID();
    activeSearchId.current = searchId;
    setState({ isSearching: true });
    try {
      const response = await api.searchPackageCatalog(searchId, managerId, query);
      if (activeSearchId.current === searchId) setState({ response, isSearching: false });
      return response;
    } catch (error) {
      if (activeSearchId.current === searchId) {
        setState({ isSearching: false, error: apiErrorMessage(error, "软件包目录搜索失败。") });
      }
      return undefined;
    } finally {
      if (activeSearchId.current === searchId) activeSearchId.current = undefined;
    }
  }, []);

  const cancel = useCallback(async () => {
    if (!activeSearchId.current) return;
    await api.cancelPackageCatalogSearch(activeSearchId.current);
  }, []);

  const clear = useCallback(() => setState({ isSearching: false }), []);

  return { ...state, search, cancel, clear };
}
