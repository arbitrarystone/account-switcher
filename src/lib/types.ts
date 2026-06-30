/** 与 Rust 后端 `account` 模块对应的类型（camelCase 序列化）。 */

export type Tool = "claude" | "codex";

export interface Account {
  id: string;
  name: string;
  tool: Tool;
  baseUrl: string;
  model?: string;
  token: string;
  tags?: string[];
  extraArgs?: string[];
  createdAt: string;
  updatedAt: string;
}

/** 新建账号输入（含明文 Token，仅创建时传递）。 */
export interface NewAccount {
  name: string;
  tool: Tool;
  baseUrl: string;
  model?: string;
  token: string;
  tags?: string[];
  extraArgs?: string[];
}

/**
 * 更新账号输入。仅包含要修改的字段：
 * - 省略字段 = 不变
 * - `model: null` / `tags: null` = 清空
 * - `token` 提供 = 同步更新钥匙串
 */
export interface AccountUpdate {
  name?: string;
  baseUrl?: string;
  model?: string | null;
  token?: string;
  tags?: string[] | null;
  extraArgs?: string[] | null;
}

/** 后端 `AccountError` 的序列化形态。 */
export interface AccountError {
  kind: "NotFound" | "Validation" | "Storage";
  message: string;
}

/** 持久化的会话记录（与后端 session 模块对应）。 */
export interface SessionRecord {
  accountId: string;
  tool: Tool;
  projectDir: string;
  title: string;
  lastUsedAt: string;
  open: boolean;
}

export const TOOL_LABELS: Record<Tool, string> = {
  claude: "Claude Code",
  codex: "Codex",
};
