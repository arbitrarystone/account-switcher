import { useCallback, useEffect, useState } from "react";

import { errorMessage, usageApi, type TokenUsagePoint } from "../lib/api";

interface UseTokenUsageResult {
  points: TokenUsagePoint[];
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

/** 按时间范围（+可选账号过滤）拉取 token 用量序列；range/账号变化时自动重拉。 */
export function useTokenUsage(
  startDate: string,
  endDate: string,
  accountId?: string,
): UseTokenUsageResult {
  const [points, setPoints] = useState<TokenUsagePoint[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      setPoints(await usageApi.series(startDate, endDate, accountId));
      setError(null);
    } catch (e: unknown) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }, [startDate, endDate, accountId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { points, loading, error, refresh };
}
