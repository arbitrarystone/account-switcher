import { useCallback, useRef, useState } from "react";

import TerminalView from "./TerminalView";
import type { Tool } from "../../lib/types";

import "./terminal.css";

export interface TerminalSession {
  id: string;
  accountId: string;
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

const DRAG_MIME = "text/accsw-session";
/** 分屏比例的可调范围（%），避免拖到不可用的极端宽度。 */
const MIN_RATIO = 20;
const MAX_RATIO = 80;

/**
 * 多标签终端：每个会话一个标签 + 常驻 TerminalView。
 * 非激活面板用 CSS 隐藏（保活，避免丢失终端内容），激活时 TerminalView 内的
 * ResizeObserver 会自动 refit。
 *
 * 分屏：把标签拖到终端区，落在左/右半屏即把该会话放到对应侧（左右并排、
 * 中缝可拖动调宽）。把右侧会话拖回左半屏、或关闭右侧会话即取消分屏。
 */
function TerminalTabs({ sessions, activeId, onActivate, onClose }: TerminalTabsProps) {
  const noopExit = useCallback(() => {}, []);
  const [splitId, setSplitId] = useState<string | null>(null);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [dropHint, setDropHint] = useState<"left" | "right" | null>(null);
  const [splitRatio, setSplitRatio] = useState(50);
  const panesRef = useRef<HTMLDivElement>(null);

  // 会话被关掉、或与激活标签重合时，分屏自动失效（渲染期守卫，不留脏状态）。
  const effectiveSplitId =
    splitId && splitId !== activeId && sessions.some((s) => s.id === splitId)
      ? splitId
      : null;

  /** 把会话放到某一侧。右侧 = 分屏；左侧 = 激活（若它原本在右侧则取消分屏）。 */
  const placeSession = (id: string, side: "left" | "right") => {
    if (side === "right") {
      if (sessions.length < 2) return; // 只有一个会话没法分
      if (id === activeId) {
        // 把当前激活的放到右边：左边自动切到下一个会话
        const other = sessions.find((s) => s.id !== id);
        if (!other) return;
        onActivate(other.id);
      }
      setSplitId(id);
    } else {
      onActivate(id);
      if (id === splitId) setSplitId(null);
    }
  };

  const handleDrop = (e: React.DragEvent, side: "left" | "right") => {
    e.preventDefault();
    const id = e.dataTransfer.getData(DRAG_MIME);
    setDraggingId(null);
    setDropHint(null);
    if (id && sessions.some((s) => s.id === id)) placeSession(id, side);
  };

  /** 中缝拖动调整分屏比例（pointer capture，指针移出也不丢）。 */
  const handleDividerPointerDown = (e: React.PointerEvent) => {
    const container = panesRef.current;
    if (!container) return;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const rect = container.getBoundingClientRect();
    const onMove = (ev: PointerEvent) => {
      const pct = ((ev.clientX - rect.left) / rect.width) * 100;
      setSplitRatio(Math.min(MAX_RATIO, Math.max(MIN_RATIO, pct)));
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  return (
    <div className="terminal-tabs">
      <div className="tab-bar" role="tablist">
        {sessions.map((s) => (
          <div
            key={s.id}
            className={
              "tab" +
              (s.id === activeId ? " is-active" : "") +
              (s.id === effectiveSplitId ? " is-split" : "")
            }
            onClick={() => onActivate(s.id)}
            role="tab"
            aria-selected={s.id === activeId}
            title={`${s.projectDir}（拖到终端区左/右半屏可分屏）`}
            draggable
            onDragStart={(e) => {
              e.dataTransfer.setData(DRAG_MIME, s.id);
              e.dataTransfer.effectAllowed = "move";
              setDraggingId(s.id);
            }}
            onDragEnd={() => {
              setDraggingId(null);
              setDropHint(null);
            }}
          >
            <span className="account-dot" data-tool={s.tool} aria-hidden="true" />
            <span className="tab-label">{s.accountName}</span>
            {s.id === effectiveSplitId && (
              <span className="tab-split-badge" title="在右侧分屏" aria-hidden="true">
                ◫
              </span>
            )}
            <button
              className="tab-close"
              title="结束会话"
              onClick={(e) => {
                e.stopPropagation();
                if (s.id === splitId) setSplitId(null);
                onClose(s.id);
              }}
            >
              ✕
            </button>
          </div>
        ))}
      </div>
      <div
        ref={panesRef}
        className={"tab-panes" + (effectiveSplitId ? " is-split" : "")}
        style={{ "--split-ratio": splitRatio } as React.CSSProperties}
      >
        {sessions.map((s) => (
          <div
            key={s.id}
            className={
              "tab-pane" +
              (s.id === activeId ? " is-active" : "") +
              (s.id === effectiveSplitId ? " is-split-pane" : "")
            }
          >
            <TerminalView sessionId={s.id} onExit={noopExit} />
          </div>
        ))}
        {effectiveSplitId && (
          <div
            className="split-divider"
            role="separator"
            aria-orientation="vertical"
            title="拖动调整分屏比例"
            onPointerDown={handleDividerPointerDown}
          />
        )}
        {draggingId && (
          <div className="drop-zones">
            <div
              className={"drop-zone" + (dropHint === "left" ? " is-hover" : "")}
              onDragOver={(e) => {
                e.preventDefault();
                setDropHint("left");
              }}
              onDragLeave={() => setDropHint(null)}
              onDrop={(e) => handleDrop(e, "left")}
            >
              <span className="drop-zone-label">左半屏</span>
            </div>
            <div
              className={"drop-zone" + (dropHint === "right" ? " is-hover" : "")}
              onDragOver={(e) => {
                e.preventDefault();
                setDropHint("right");
              }}
              onDragLeave={() => setDropHint(null)}
              onDrop={(e) => handleDrop(e, "right")}
            >
              <span className="drop-zone-label">右半屏</span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

export default TerminalTabs;
