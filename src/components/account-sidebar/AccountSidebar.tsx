import type { ToolDefaults, UsageSummary } from "../../lib/api";
import { formatDuration } from "../../lib/format";
import type { Account, Tool } from "../../lib/types";
import { TOOL_LABELS } from "../../lib/types";

interface AccountSidebarProps {
  accounts: Account[];
  loading: boolean;
  selectedId: string | null;
  defaults: ToolDefaults;
  usageByAccount: Map<string, UsageSummary>;
  runningAccountIds: Set<string>;
  onSelect: (id: string) => void;
  onAdd: () => void;
  onEdit: (account: Account) => void;
  onClone: (account: Account) => void;
  onDelete: (account: Account) => void;
  onSetDefault: (account: Account) => void;
  onClearDefault: (account: Account) => void;
}

const TOOLS: readonly Tool[] = ["claude", "codex"];

function AccountSidebar({
  accounts,
  loading,
  selectedId,
  defaults,
  usageByAccount,
  runningAccountIds,
  onSelect,
  onAdd,
  onEdit,
  onClone,
  onDelete,
  onSetDefault,
  onClearDefault,
}: AccountSidebarProps) {
  return (
    <aside className="account-sidebar">
      <div className="sidebar-head">
        <h2 className="sidebar-title">账号</h2>
        <button className="btn btn-ghost btn-icon" title="新建账号" onClick={onAdd}>
          ＋
        </button>
      </div>

      {loading ? (
        <div className="sidebar-empty">加载中…</div>
      ) : accounts.length === 0 ? (
        <div className="sidebar-empty">
          还没有账号
          <br />
          点右上角 <strong>＋</strong> 新建
        </div>
      ) : (
        <nav className="account-list" aria-label="账号列表">
          {TOOLS.map((tool) => {
            const group = accounts.filter((a) => a.tool === tool);
            if (group.length === 0) return null;
            return (
              <section key={tool} className="account-group">
                <h3 className="group-label">{TOOL_LABELS[tool]}</h3>
                {group.map((account) => {
                  const isDefault = defaults[account.tool] === account.id;
                  const isRunning = runningAccountIds.has(account.id);
                  const u = usageByAccount.get(account.id);
                  return (
                    <div
                      key={account.id}
                      className="account-item"
                      aria-current={selectedId === account.id}
                      onClick={() => onSelect(account.id)}
                    >
                      <span
                        className="account-dot"
                        data-tool={account.tool}
                        aria-hidden="true"
                      />
                      <span className="account-main">
                        <span className="account-name" title={account.baseUrl}>
                          {account.name}
                        </span>
                        <span className="account-usage">
                          {isRunning && (
                            <span className="usage-live" aria-hidden="true" />
                          )}
                          {isRunning && "运行中 · "}
                          {u && u.sessions > 0
                            ? `${u.sessions} 次 · 累计 ${formatDuration(u.totalDurationSec)}`
                            : "尚未使用"}
                        </span>
                      </span>
                      {isDefault && (
                        <span className="account-default" title="全局默认">
                          ★
                        </span>
                      )}
                      <span className="account-actions">
                        <button
                          className="account-action"
                          title={isDefault ? "取消全局默认" : "设为全局默认"}
                          onClick={(e) => {
                            e.stopPropagation();
                            if (isDefault) onClearDefault(account);
                            else onSetDefault(account);
                          }}
                        >
                          {isDefault ? "★" : "☆"}
                        </button>
                        <button
                          className="account-action"
                          title="编辑"
                          onClick={(e) => {
                            e.stopPropagation();
                            onEdit(account);
                          }}
                        >
                          ✎
                        </button>
                        <button
                          className="account-action"
                          title="克隆到另一工具"
                          onClick={(e) => {
                            e.stopPropagation();
                            onClone(account);
                          }}
                        >
                          ⎘
                        </button>
                        <button
                          className="account-action account-action-danger"
                          title="删除"
                          onClick={(e) => {
                            e.stopPropagation();
                            onDelete(account);
                          }}
                        >
                          ✕
                        </button>
                      </span>
                    </div>
                  );
                })}
              </section>
            );
          })}
        </nav>
      )}
    </aside>
  );
}

export default AccountSidebar;
