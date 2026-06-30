import { useCallback, useEffect, useState } from "react";

import { accountApi, errorMessage } from "../lib/api";
import type { Account, AccountUpdate, NewAccount, Tool } from "../lib/types";

interface UseAccountsResult {
  accounts: Account[];
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  create: (input: NewAccount) => Promise<Account>;
  update: (id: string, patch: AccountUpdate) => Promise<Account>;
  remove: (id: string) => Promise<void>;
  clone: (id: string, targetTool: Tool) => Promise<Account>;
}

/** 账号数据状态管理：加载、错误、CRUD（变更后自动刷新列表）。 */
export function useAccounts(): UseAccountsResult {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      setAccounts(await accountApi.list());
      setError(null);
    } catch (e: unknown) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const create = useCallback(
    async (input: NewAccount) => {
      const created = await accountApi.create(input);
      await refresh();
      return created;
    },
    [refresh],
  );

  const update = useCallback(
    async (id: string, patch: AccountUpdate) => {
      const updated = await accountApi.update(id, patch);
      await refresh();
      return updated;
    },
    [refresh],
  );

  const remove = useCallback(
    async (id: string) => {
      await accountApi.remove(id);
      await refresh();
    },
    [refresh],
  );

  const clone = useCallback(
    async (id: string, targetTool: Tool) => {
      const cloned = await accountApi.clone(id, targetTool);
      await refresh();
      return cloned;
    },
    [refresh],
  );

  return { accounts, loading, error, refresh, create, update, remove, clone };
}
