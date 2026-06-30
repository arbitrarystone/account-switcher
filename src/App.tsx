import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import AccountFormDialog from "./components/account-sidebar/AccountFormDialog";
import AccountSidebar from "./components/account-sidebar/AccountSidebar";
import TerminalView from "./components/terminal-tabs/TerminalView";
import { useAccounts } from "./hooks/useAccounts";
import { errorMessage, sessionApi } from "./lib/api";
import type { Account, Tool } from "./lib/types";
import { TOOL_LABELS } from "./lib/types";

interface DialogState {
  mode: "create" | "edit";
  initial: Account | null;
}

interface SessionState {
  id: string;
  accountName: string;
  projectDir: string;
  tool: Tool;
}

function App() {
  const accounts = useAccounts();
  const [version, setVersion] = useState("…");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [dialog, setDialog] = useState<DialogState | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<Account | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  // 起任务条状态
  const [projectDir, setProjectDir] = useState("");
  const [launchAccountId, setLaunchAccountId] = useState("");
  const [launching, setLaunching] = useState(false);
  const [session, setSession] = useState<SessionState | null>(null);

  useEffect(() => {
    invoke<string>("app_version")
      .then(setVersion)
      .catch(() => setVersion("offline"));
  }, []);

  const selected = accounts.accounts.find((a) => a.id === selectedId) ?? null;
  const banner = actionError ?? accounts.error;
  const canLaunch = Boolean(projectDir && launchAccountId && !launching && !session);

  const handlePickDir = async () => {
    try {
      const dir = await open({ directory: true, title: "选择项目目录" });
      if (typeof dir === "string") setProjectDir(dir);
    } catch (e: unknown) {
      setActionError(errorMessage(e));
    }
  };

  const handleLaunch = async () => {
    if (!launchAccountId || !projectDir) return;
    setLaunching(true);
    setActionError(null);
    try {
      const sid = await sessionApi.launch({
        accountId: launchAccountId,
        projectDir,
        rows: 24,
        cols: 80,
      });
      const acc = accounts.accounts.find((a) => a.id === launchAccountId);
      setSession({
        id: sid,
        accountName: acc?.name ?? "",
        projectDir,
        tool: acc?.tool ?? "claude",
      });
    } catch (e: unknown) {
      setActionError(errorMessage(e));
    } finally {
      setLaunching(false);
    }
  };

  const handleCloseSession = async () => {
    if (!session) return;
    try {
      await sessionApi.close(session.id);
    } catch {
      /* 关闭失败可忽略 */
    }
    setSession(null);
  };

  const handleSessionExit = useCallback(() => {
    // 进程已退出，终端内已提示退出码；保留视图供查看，用户手动结束会话。
  }, []);

  const handleClone = async (account: Account) => {
    setActionError(null);
    const target: Tool = account.tool === "claude" ? "codex" : "claude";
    try {
      await accounts.clone(account.id, target);
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
    } catch (e: unknown) {
      setActionError(errorMessage(e));
    }
  };

  return (
    <div className="app-shell">
      {/* ── 顶部：起任务条 ─────────────────────────────── */}
      <header className="launch-bar">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true" />
          <span className="brand-name">Account Switcher</span>
        </div>
        <div className="launch-fields">
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
              <button
                className="btn btn-ghost btn-sm"
                onClick={handlePickDir}
                disabled={!!session}
              >
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
              disabled={!!session || accounts.accounts.length === 0}
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
          <button className="btn btn-primary" onClick={handleLaunch} disabled={!canLaunch}>
            {launching ? "起任务中…" : "起任务"}
          </button>
        </div>
      </header>

      {/* ── 工作区：账号侧栏 + 终端/详情区 ──────────────── */}
      <div className="workbench">
        <AccountSidebar
          accounts={accounts.accounts}
          loading={accounts.loading}
          selectedId={selectedId}
          onSelect={setSelectedId}
          onAdd={() => setDialog({ mode: "create", initial: null })}
          onEdit={(a) => setDialog({ mode: "edit", initial: a })}
          onClone={handleClone}
          onDelete={(a) => setDeleteTarget(a)}
        />

        <main className={"terminal-area" + (session ? " has-session" : "")}>
          {session ? (
            <div className="session-pane">
              <div className="session-head">
                <span className="account-dot" data-tool={session.tool} aria-hidden="true" />
                <span className="session-title">{session.accountName}</span>
                <span className="session-path" title={session.projectDir}>
                  {session.projectDir}
                </span>
                <button className="btn btn-ghost btn-sm" onClick={handleCloseSession}>
                  结束会话
                </button>
              </div>
              <TerminalView sessionId={session.id} onExit={handleSessionExit} />
            </div>
          ) : selected ? (
            <div className="account-detail">
              <div className="detail-head">
                <span className="account-dot" data-tool={selected.tool} aria-hidden="true" />
                <h2 className="detail-name">{selected.name}</h2>
                <span className="detail-tool">{TOOL_LABELS[selected.tool]}</span>
              </div>
              <dl className="detail-grid">
                <dt>BASE_URL</dt>
                <dd className="detail-mono">{selected.baseUrl}</dd>
                <dt>默认模型</dt>
                <dd>{selected.model ?? "—"}</dd>
                <dt>Token</dt>
                <dd>🔒 已安全保存于系统钥匙串</dd>
                <dt>标签</dt>
                <dd>{selected.tags?.length ? selected.tags.join("、") : "—"}</dd>
                <dt>创建时间</dt>
                <dd className="detail-mono">
                  {new Date(selected.createdAt).toLocaleString()}
                </dd>
              </dl>
              <p className="detail-hint">
                在顶部选择项目目录后点「起任务」，即可用此账号开一个隔离终端会话。
              </p>
            </div>
          ) : (
            <div className="empty-state">
              <div className="empty-glyph" aria-hidden="true">
                ⌘
              </div>
              <p className="empty-title">选择或新建一个账号</p>
              <p className="empty-hint">
                左侧管理你的 <strong>Claude Code / Codex</strong> 中转账号；
                Token 安全存于系统钥匙串，绝不落明文。
              </p>
            </div>
          )}
        </main>
      </div>

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
            {session ? "会话运行中" : `就绪 · ${accounts.accounts.length} 个账号`}
          </span>
        )}
        <span className="status-spacer" />
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
    </div>
  );
}

export default App;
