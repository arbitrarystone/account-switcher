import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { Account, AccountUpdate, NewAccount, Tool } from "./types";

/**
 * 账号相关 Tauri 命令封装。参数名与 Rust 命令签名一一对应。
 */
export const accountApi = {
  list: () => invoke<Account[]>("account_list"),
  create: (input: NewAccount) => invoke<Account>("account_create", { input }),
  get: (id: string) => invoke<Account>("account_get", { id }),
  update: (id: string, patch: AccountUpdate) =>
    invoke<Account>("account_update", { id, patch }),
  remove: (id: string) => invoke<void>("account_delete", { id }),
  clone: (id: string, targetTool: Tool) =>
    invoke<Account>("account_clone", { id, targetTool }),
};

// ── PTY 会话 ────────────────────────────────────────────

export interface PtyOutput {
  sessionId: string;
  data: string;
}

export interface PtyExit {
  sessionId: string;
  code: number;
}

export const sessionApi = {
  /** 起一个隔离终端会话，返回 sessionId。 */
  launch: (params: {
    accountId: string;
    projectDir: string;
    rows: number;
    cols: number;
  }) => invoke<string>("launch_session", params),
  write: (sessionId: string, data: string) =>
    invoke<void>("pty_write", { sessionId, data }),
  resize: (sessionId: string, rows: number, cols: number) =>
    invoke<void>("pty_resize", { sessionId, rows, cols }),
  close: (sessionId: string) => invoke<void>("pty_close", { sessionId }),
  onOutput: (cb: (o: PtyOutput) => void): Promise<UnlistenFn> =>
    listen<PtyOutput>("pty://output", (e) => cb(e.payload)),
  onExit: (cb: (o: PtyExit) => void): Promise<UnlistenFn> =>
    listen<PtyExit>("pty://exit", (e) => cb(e.payload)),
};

// ── 全局默认（M4）────────────────────────────────────────

export interface ToolDefaults {
  claude: string | null;
  codex: string | null;
}

export const defaultsApi = {
  get: () => invoke<ToolDefaults>("get_defaults"),
  set: (tool: Tool, accountId: string) =>
    invoke<void>("set_default", { tool, accountId }),
  clear: (tool: Tool) => invoke<void>("clear_default", { tool }),
};

/** 把后端错误（`AccountError` 对象或字符串）转为可展示文案。 */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return "发生未知错误";
}
