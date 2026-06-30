import { useCallback } from "react";

import TerminalView from "./TerminalView";
import type { Tool } from "../../lib/types";

import "./terminal.css";

export interface TerminalSession {
  id: string;
  accountName: string;
  projectDir: string;
  tool: Tool;
}

interface TerminalTabsProps {
  sessions: TerminalSession[];
  activeId: string | null;
  onActivate: (id: string) => void;
  onClose: (id: string) => void;
}

/**
 * 多标签终端：每个会话一个标签 + 常驻 TerminalView。
 * 非激活面板用 CSS 隐藏（保活，避免丢失终端内容），激活时 TerminalView 内的
 * ResizeObserver 会自动 refit。
 */
function TerminalTabs({ sessions, activeId, onActivate, onClose }: TerminalTabsProps) {
  const noopExit = useCallback(() => {}, []);

  return (
    <div className="terminal-tabs">
      <div className="tab-bar" role="tablist">
        {sessions.map((s) => (
          <div
            key={s.id}
            className={"tab" + (s.id === activeId ? " is-active" : "")}
            onClick={() => onActivate(s.id)}
            role="tab"
            aria-selected={s.id === activeId}
            title={s.projectDir}
          >
            <span className="account-dot" data-tool={s.tool} aria-hidden="true" />
            <span className="tab-label">{s.accountName}</span>
            <button
              className="tab-close"
              title="结束会话"
              onClick={(e) => {
                e.stopPropagation();
                onClose(s.id);
              }}
            >
              ✕
            </button>
          </div>
        ))}
      </div>
      <div className="tab-panes">
        {sessions.map((s) => (
          <div
            key={s.id}
            className={"tab-pane" + (s.id === activeId ? " is-active" : "")}
          >
            <TerminalView sessionId={s.id} onExit={noopExit} />
          </div>
        ))}
      </div>
    </div>
  );
}

export default TerminalTabs;
