import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import AccountFormDialog from "./components/account-sidebar/AccountFormDialog";
import AccountSidebar from "./components/account-sidebar/AccountSidebar";
import EyeToggle from "./components/ui/EyeToggle";
import TerminalTabs, { type TerminalSession } from "./components/terminal-tabs/TerminalTabs";
import UpdateChecker from "./components/updater/UpdateChecker";

// recharts 体积不小，懒加载：只有真的点进用量统计页才拉取，不拖慢启动。
const UsageDashboard = lazy(() => import("./components/usage-dashboard/UsageDashboard"));
import { useAccounts } from "./hooks/useAccounts";
import {
  defaultsApi,
  errorMessage,
  memoryApi,
  sessionApi,
  usageApi,
  type ToolDefaults,
  type UsageSummary,
} from "./lib/api";
import { formatDuration } from "./lib/format";
import type { Account, SessionRecord, Tool } from "./lib/types";
import { TOOL_LABELS } from "./lib/types";

interface DialogState {
  mode: "create" | "edit";
  initial: Account | null;
}

const EMPTY_DEFAULTS: ToolDefaults = { claude: null, codex: null };

function App() {
  const accounts = useAccounts();
  const [version, setVersion] = useState("…");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [dialog, setDialog] = useState<DialogState | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<Account | null>(null);
  const [setDefaultTarget, setSetDefaultTarget] = useState<Account | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [defaults, setDefaults] = useState<ToolDefaults>(EMPTY_DEFAULTS);
  const [showDetailToken, setShowDetailToken] = useState(false);
  const [usage, setUsage] = useState<UsageSummary[]>([]);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [view, setView] = useState<"workbench" | "usage">("workbench");

  // 起任务条 + 多会话
  const [projectDir, setProjectDir] = useState("");
  const [launchAccountId, setLaunchAccountId] = useState("");
  const [launching, setLaunching] = useState(false);
  const [skipPermissions, setSkipPermissions] = useState(false);
  const [sessions, setSessions] = useState<TerminalSession[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [history, setHistory] = useState<SessionRecord[]>([]);
  const restoredRef = useRef(false);

  const loadDefaults = useCallback(() => {
    defaultsApi
      .get()
      .then(setDefaults)
      .catch(() => setDefaults(EMPTY_DEFAULTS));
  }, []);

  const loadUsage = useCallback(() => {
    usageApi
      .summary()
      .then(setUsage)
      .catch(() => setUsage([]));
  }, []);

  const loadHistory = useCallback(() => {
    sessionApi
      .history()
      .then(setHistory)
      .catch(() => setHistory([]));
  }, []);

  /** 起一个会话标签（普通起任务 / 自动恢复 / 历史重起共用）。 */
  const launchInto = useCallback(
    async (
      accountId: string,
      tool: Tool,
      accountName: string,
      dir: string,
      opts: { skipPermissions: boolean; resume: boolean },
    ) => {
      const sid = await sessionApi.launch({
        accountId,
        projectDir: dir,
        skipPermissions: opts.skipPermissions,
        resume: opts.resume,
        rows: 24,
        cols: 80,
      });
      setSessions((prev) => [
        ...prev,
        { id: sid, accountId, accountName, projectDir: dir, tool },
      ]);
      setActiveSessionId(sid);
      loadUsage();
      loadHistory();
    },
    [loadUsage, loadHistory],
  );

  useEffect(() => {
    invoke<string>("app_version")
      .then(setVersion)
      .catch(() => setVersion("offline"));
    loadDefaults();
    loadUsage();
    loadHistory();
  }, [loadDefaults, loadUsage, loadHistory]);

  // 重启后自动恢复上次打开的会话（账号仍存在时），仅执行一次。
  useEffect(() => {
    if (restoredRef.current || accounts.loading) return;
    restoredRef.current = true;
    void (async () => {
      try {
        const open = await sessionApi.openSessions();
        for (const s of open) {
          const acc = accounts.accounts.find((a) => a.id === s.accountId);
          if (!acc) continue;
          try {
            await launchInto(s.accountId, s.tool, acc.name, s.projectDir, {
              skipPermissions: s.skipPermissions,
              resume: true,
            });
          } catch {
            /* 单个会话恢复失败不影响其他 */
          }
        }
      } catch {
        /* 无可恢复会话 */
      }
    })();
  }, [accounts.loading, accounts.accounts, launchInto]);

  const selected = accounts.accounts.find((a) => a.id === selectedId) ?? null;
  const selectedUsage = usage.find((u) => u.accountId === selectedId) ?? null;
  const usageByAccount = new Map(usage.map((u) => [u.accountId, u]));
  const runningAccountIds = new Set(sessions.map((s) => s.accountId));
  const banner = actionError ?? accounts.error;
  const canLaunch = Boolean(projectDir && launchAccountId && !launching);

  const handlePickDir = async () => {
    try {
      const dir = await open({ directory: true, title: "选择项目目录" });
      if (typeof dir !== "string") return;
      setProjectDir(dir);
      // 记忆：预填该项目上次用过的账号
      const [lastClaude, lastCodex] = await Promise.all([
        memoryApi.getLast(dir, "claude"),
        memoryApi.getLast(dir, "codex"),
      ]);
      const last = lastClaude ?? lastCodex;
      if (last && accounts.accounts.some((a) => a.id === last)) {
        setLaunchAccountId(last);
      }
    } catch (e: unknown) {
      setActionError(errorMessage(e));
    }
  };

  const handleLaunch = async () => {
    if (!launchAccountId || !projectDir) return;
    const acc = accounts.accounts.find((a) => a.id === launchAccountId);
    if (!acc) return;
    setLaunching(true);
    setActionError(null);
    try {
      await launchInto(acc.id, acc.tool, acc.name, projectDir, {
        skipPermissions,
        resume: false,
      });
    } catch (e: unknown) {
      setActionError(errorMessage(e));
    } finally {
      setLaunching(false);
    }
  };

  const handleCloseSession = async (id: string) => {
    const sess = sessions.find((s) => s.id === id);
    try {
      await sessionApi.close(id);
    } catch {
      /* 关闭失败可忽略 */
    }
    if (sess) {
      try {
        await sessionApi.markClosed(sess.accountId, sess.projectDir);
      } catch {
        /* 忽略 */
      }
    }
    const next = sessions.filter((s) => s.id !== id);
    setSessions(next);
    if (id === activeSessionId) {
      setActiveSessionId(next.length ? next[next.length - 1].id : null);
    }
    loadUsage();
    loadHistory();
  };

  /** 从历史重起会话（resume 续接对话）；已打开则跳过。 */
  const handleRestoreSession = async (s: SessionRecord) => {
    const acc = accounts.accounts.find((a) => a.id === s.accountId);
    if (!acc) {
      setActionError("该会话的账号已被删除");
      return;
    }
    if (
      sessions.some(
        (t) => t.accountId === s.accountId && t.projectDir === s.projectDir,
      )
    ) {
      return;
    }
    setActionError(null);
    try {
      await launchInto(acc.id, acc.tool, acc.name, s.projectDir, {
        skipPermissions: s.skipPermissions,
        resume: true,
      });
    } catch (e: unknown) {
      setActionError(errorMessage(e));
    }
  };

  const handleRemoveHistory = async (s: SessionRecord) => {
    try {
      await sessionApi.removeHistory(s.accountId, s.projectDir);
      loadHistory();
    } catch (e: unknown) {
      setActionError(errorMessage(e));
    }
  };

  const handleClone = async (account: Account) => {
    setActionError(null);
    const target: Tool = account.tool === "claude" ? "codex" : "claude";
    try {
      await accounts.clone(account.id, target);
    } catch (e: unknown) {
      setActionError(errorMessage(e));
    }
  };

  const handleClearDefault = async (account: Account) => {
    setActionError(null);
    try {
      await defaultsApi.clear(account.tool);
      loadDefaults();
    } catch (e: unknown) {
      setActionError(errorMessage(e));
    }
  };

  const confirmSetDefault = async () => {
    if (!setDefaultTarget) return;
    setActionError(null);
    try {
      await defaultsApi.set(setDefaultTarget.tool, setDefaultTarget.id);
      loadDefaults();
      setSetDefaultTarget(null);
    } catch (e: unknown) {
      setActionError(errorMessage(e));
    }
  };

  const confirmDelete = async () => {
    if (!deleteTarget) return;
    setActionError(null);
    try {
      await accounts.remove(deleteTarget.id);
      if (selectedId === deleteTarget.id) setSelectedId(null);
      if (launchAccountId === deleteTarget.id) setLaunchAccountId("");
      setDeleteTarget(null);
      loadDefaults();
    } catch (e: unknown) {
      setActionError(errorMessage(e));
    }
  };

  return (
    <div className="app-shell">
      {/* ── 顶部：起任务条 ─────────────────────────────── */}
      <header className="launch-bar">
        <div className="brand">
          <button
            className="sidebar-toggle"
            onClick={() => setSidebarCollapsed((c) => !c)}
            title={sidebarCollapsed ? "展开账号栏" : "收起账号栏"}
            aria-label="切换账号栏"
          >
            {sidebarCollapsed ? "»" : "«"}
          </button>
          <span className="brand-mark" aria-hidden="true" />
          <span className="brand-name">Account Switcher</span>
        </div>
        {view === "workbench" && <div className="launch-fields">
          <label className="field">
            <span className="field-label">项目目录</span>
            <div className="field-input-row">
              <input
                className="field-input field-path"
                value={projectDir}
                placeholder="未选择"
                title={projectDir}
                readOnly
              />
              <button className="btn btn-ghost btn-sm" onClick={handlePickDir}>
                选择…
              </button>
            </div>
          </label>
          <label className="field">
            <span className="field-label">账号</span>
            <select
              className="field-input"
              value={launchAccountId}
              onChange={(e) => setLaunchAccountId(e.target.value)}
              disabled={accounts.accounts.length === 0}
            >
              <option value="">
                {accounts.accounts.length ? "选择账号…" : "暂无账号"}
              </option>
              {accounts.accounts.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name} · {TOOL_LABELS[a.tool]}
                </option>
              ))}
            </select>
          </label>
          <label
            className="skip-perm"
            title="claude 加 --dangerously-skip-permissions；codex 加 --dangerously-bypass-approvals-and-sandbox"
          >
            <input
              type="checkbox"
              checked={skipPermissions}
              onChange={(e) => setSkipPermissions(e.target.checked)}
            />
            <span>跳过权限</span>
          </label>
          <button className="btn btn-primary" onClick={handleLaunch} disabled={!canLaunch}>
            {launching ? "起任务中…" : "起任务"}
          </button>
        </div>}
        <button
          className="btn btn-ghost btn-sm nav-usage-btn"
          onClick={() => setView((v) => (v === "usage" ? "workbench" : "usage"))}
        >
          {view === "usage" ? "返回工作台" : "用量统计"}
        </button>
      </header>

      {view === "usage" ? (
        <Suspense fallback={<div className="usage-empty">加载用量统计…</div>}>
          <UsageDashboard accounts={accounts.accounts} onClose={() => setView("workbench")} />
        </Suspense>
      ) : (
      /* ── 工作区：账号侧栏 + 终端/详情区 ──────────────── */
      <div className={"workbench" + (sidebarCollapsed ? " sidebar-collapsed" : "")}>
        <AccountSidebar
          accounts={accounts.accounts}
          loading={accounts.loading}
          selectedId={selectedId}
          defaults={defaults}
          usageByAccount={usageByAccount}
          runningAccountIds={runningAccountIds}
          onSelect={setSelectedId}
          onAdd={() => setDialog({ mode: "create", initial: null })}
          onEdit={(a) => setDialog({ mode: "edit", initial: a })}
          onClone={handleClone}
          onDelete={(a) => setDeleteTarget(a)}
          onSetDefault={(a) => setSetDefaultTarget(a)}
          onClearDefault={handleClearDefault}
        />

        <main className={"terminal-area" + (sessions.length ? " has-session" : "")}>
          {sessions.length > 0 ? (
            <TerminalTabs
              sessions={sessions}
              activeId={activeSessionId}
              onActivate={setActiveSessionId}
              onClose={handleCloseSession}
            />
          ) : selected ? (
            <div className="account-detail">
              <div className="detail-head">
                <span className="account-dot" data-tool={selected.tool} aria-hidden="true" />
                <h2 className="detail-name">{selected.name}</h2>
                <span className="detail-tool">{TOOL_LABELS[selected.tool]}</span>
                {defaults[selected.tool] === selected.id && (
                  <span className="detail-default" title="全局默认">
                    ★ 默认
                  </span>
                )}
              </div>
              <dl className="detail-grid">
                <dt>BASE_URL</dt>
                <dd className="detail-mono">{selected.baseUrl}</dd>
                <dt>默认模型</dt>
                <dd>{selected.model ?? "—"}</dd>
                <dt>Token</dt>
                <dd className="token-reveal">
                  {selected.token ? (
                    <>
                      <span className="detail-mono">
                        {showDetailToken ? selected.token : "••••••••••••"}
                      </span>
                      <EyeToggle
                        shown={showDetailToken}
                        onToggle={() => setShowDetailToken((s) => !s)}
                      />
                    </>
                  ) : (
                    <span className="token-missing">
                      ⚠ 未设置 —— 请编辑账号填入 Token（否则起任务无法使用该账号）
                    </span>
                  )}
                </dd>
                <dt>标签</dt>
                <dd>{selected.tags?.length ? selected.tags.join("、") : "—"}</dd>
                <dt>用量</dt>
                <dd>
                  {selectedUsage
                    ? `${selectedUsage.sessions} 次会话 · 累计 ${formatDuration(selectedUsage.totalDurationSec)}`
                    : "暂无记录"}
                </dd>
                <dt>创建时间</dt>
                <dd className="detail-mono">
                  {new Date(selected.createdAt).toLocaleString()}
                </dd>
              </dl>
              <p className="detail-hint">
                选项目目录后点「起任务」开隔离终端会话；可重复起任务多账号并发。
                ☆ 设为全局默认后，app 外的终端也会跟随此账号。
              </p>
            </div>
          ) : (
            <div className="empty-state">
              <div className="empty-glyph" aria-hidden="true">
                ⌘
              </div>
              <p className="empty-title">选择账号起任务，或恢复历史会话</p>
              <p className="empty-hint">
                左侧管理 <strong>Claude Code / Codex</strong> 中转账号；
                顶部选项目 + 账号点「起任务」开隔离终端。
              </p>
              {history.length > 0 && (
                <div className="session-history">
                  <h3 className="history-title">最近会话</h3>
                  <ul className="history-list">
                    {history.slice(0, 8).map((s) => (
                      <li
                        key={`${s.accountId}::${s.projectDir}`}
                        className="history-item"
                      >
                        <button
                          className="history-main"
                          onClick={() => handleRestoreSession(s)}
                          title={`重起并续接：${s.projectDir}`}
                        >
                          <span
                            className="account-dot"
                            data-tool={s.tool}
                            aria-hidden="true"
                          />
                          <span className="history-name">{s.title}</span>
                          {s.open && <span className="history-badge">上次打开</span>}
                        </button>
                        <button
                          className="history-remove"
                          onClick={() => handleRemoveHistory(s)}
                          title="从历史移除"
                        >
                          ✕
                        </button>
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
          )}
        </main>
      </div>
      )}

      {/* ── 底部：状态栏 ─────────────────────────────── */}
      <footer className="status-bar">
        {banner ? (
          <span className="status-item status-error">
            <span className="status-dot" data-state="error" aria-hidden="true" />
            {banner}
          </span>
        ) : (
          <span className="status-item">
            <span className="status-dot" data-state="ok" aria-hidden="true" />
            {sessions.length
              ? `${sessions.length} 个会话运行中`
              : `就绪 · ${accounts.accounts.length} 个账号`}
          </span>
        )}
        <span className="status-spacer" />
        <UpdateChecker currentVersion={version} />
        <span className="status-item status-muted">v{version}</span>
      </footer>

      {/* ── 弹窗 ─────────────────────────────────────── */}
      {dialog && (
        <AccountFormDialog
          mode={dialog.mode}
          initial={dialog.initial}
          onCancel={() => setDialog(null)}
          onCreate={async (input) => {
            await accounts.create(input);
            setDialog(null);
          }}
          onUpdate={async (id, patch) => {
            await accounts.update(id, patch);
            setDialog(null);
          }}
        />
      )}

      {deleteTarget && (
        <div className="dialog-backdrop" onClick={() => setDeleteTarget(null)}>
          <div className="dialog dialog-sm" onClick={(e) => e.stopPropagation()}>
            <div className="dialog-head">
              <h2 className="dialog-title">删除账号</h2>
            </div>
            <div className="dialog-body">
              <p>
                确定删除账号 <strong>{deleteTarget.name}</strong>？
                其 Token 也会从系统钥匙串中移除，此操作不可撤销。
              </p>
            </div>
            <div className="dialog-foot">
              <button className="btn btn-ghost" onClick={() => setDeleteTarget(null)}>
                取消
              </button>
              <button className="btn btn-danger" onClick={confirmDelete}>
                删除
              </button>
            </div>
          </div>
        </div>
      )}

      {setDefaultTarget && (
        <div className="dialog-backdrop" onClick={() => setSetDefaultTarget(null)}>
          <div className="dialog dialog-sm" onClick={(e) => e.stopPropagation()}>
            <div className="dialog-head">
              <h2 className="dialog-title">设为全局默认</h2>
            </div>
            <div className="dialog-body">
              <p>
                将账号 <strong>{setDefaultTarget.name}</strong> 设为{" "}
                {TOOL_LABELS[setDefaultTarget.tool]} 的全局默认。
              </p>
              {setDefaultTarget.tool === "claude" ? (
                <p className="form-warning">
                  ⚠️ Token 将以明文写入 <code>~/.claude/settings.json</code>，
                  app 外的终端也会跟随此账号。
                </p>
              ) : (
                <p className="form-warning">
                  ⚠️ 将写入 <code>~/.codex/config.toml</code>。Codex 的 Token 走环境变量{" "}
                  <code>ACCSW_CODEX_TOKEN</code>，app 外使用需自行在 shell 设置该变量。
                </p>
              )}
            </div>
            <div className="dialog-foot">
              <button className="btn btn-ghost" onClick={() => setSetDefaultTarget(null)}>
                取消
              </button>
              <button className="btn btn-primary" onClick={confirmSetDefault}>
                确定设为默认
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
