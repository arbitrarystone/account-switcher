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
/** 分屏比例的可调范围（%），避免拖到不可用的极端尺寸。 */
const MIN_RATIO = 20;
const MAX_RATIO = 80;

/** 分屏方向：row = 左右并排（副屏在右），col = 上下堆叠（副屏在下）。 */
interface SplitState {
  id: string;
  dir: "row" | "col";
}

/**
 * 多标签终端：每个会话一个标签 + 常驻 TerminalView。
 * 非激活面板用 CSS 隐藏（保活，避免丢失终端内容），激活时 TerminalView 内的
 * ResizeObserver 会自动 refit。
 *
 * 分屏：拖动标签（或副屏标题栏）到终端区，落点分「主屏 / 右分屏 / 下分屏」。
 * 分屏后该会话的标签从标签栏移走，由副屏自带的小标题栏承载（可拖回主屏、
 * 可取消分屏、可关闭）；中缝可拖动调比例。所有面板始终同父平级挂载，
 * 跨区移动只改 CSS——不重挂载、终端滚动缓冲不丢。
 */
function TerminalTabs({ sessions, activeId, onActivate, onClose }: TerminalTabsProps) {
  const noopExit = useCallback(() => {}, []);
  const [split, setSplit] = useState<SplitState | null>(null);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [dropHint, setDropHint] = useState<"main" | "right" | "bottom" | null>(null);
  const [splitRatio, setSplitRatio] = useState(50);
  const panesRef = useRef<HTMLDivElement>(null);

  // 会话被关掉、或与激活标签重合时，分屏自动失效（渲染期守卫，不留脏状态）。
  const effectiveSplit =
    split && split.id !== activeId && sessions.some((s) => s.id === split.id)
      ? split
      : null;
  const splitSession = effectiveSplit
    ? sessions.find((s) => s.id === effectiveSplit.id)
    : null;

  /** 把会话放到某个区域。主屏 = 激活（若它原本是副屏则取消分屏）；右/下 = 分屏。 */
  const placeSession = (id: string, zone: "main" | "right" | "bottom") => {
    if (zone === "main") {
      onActivate(id);
      if (id === split?.id) setSplit(null);
      return;
    }
    if (sessions.length < 2) return; // 只有一个会话没法分
    if (id === activeId) {
      // 把当前激活的分出去：主屏自动切到另一个会话
      const other = sessions.find((s) => s.id !== id);
      if (!other) return;
      onActivate(other.id);
    }
    setSplit({ id, dir: zone === "right" ? "row" : "col" });
  };

  const handleDrop = (e: React.DragEvent, zone: "main" | "right" | "bottom") => {
    e.preventDefault();
    const id = e.dataTransfer.getData(DRAG_MIME);
    setDraggingId(null);
    setDropHint(null);
    if (id && sessions.some((s) => s.id === id)) placeSession(id, zone);
  };

  const dragProps = (id: string) => ({
    draggable: true,
    onDragStart: (e: React.DragEvent) => {
      e.dataTransfer.setData(DRAG_MIME, id);
      e.dataTransfer.effectAllowed = "move";
      setDraggingId(id);
    },
    onDragEnd: () => {
      setDraggingId(null);
      setDropHint(null);
    },
  });

  const zoneProps = (zone: "main" | "right" | "bottom") => ({
    onDragOver: (e: React.DragEvent) => {
      e.preventDefault();
      setDropHint(zone);
    },
    onDragLeave: () => setDropHint(null),
    onDrop: (e: React.DragEvent) => handleDrop(e, zone),
  });

  /** 中缝拖动调整分屏比例（pointer capture，指针移出也不丢）。
   *  比例作用于「两块面板合计」的空间（标签栏那行不参与），故按两块
   *  面板 pointerdown 时的矩形来换算。 */
  const handleDividerPointerDown = (e: React.PointerEvent) => {
    const root = panesRef.current;
    const dir = effectiveSplit?.dir;
    if (!root || !dir) return;
    const main = root.querySelector(".tab-pane.is-active")?.getBoundingClientRect();
    const side = root.querySelector(".tab-pane.is-split-pane")?.getBoundingClientRect();
    if (!main || !side) return;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const onMove = (ev: PointerEvent) => {
      const pct =
        dir === "row"
          ? ((ev.clientX - main.left) / (main.width + side.width)) * 100
          : ((ev.clientY - main.top) / (main.height + side.height)) * 100;
      setSplitRatio(Math.min(MAX_RATIO, Math.max(MIN_RATIO, pct)));
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  // Grid 轨道随分屏模式切换：
  // 单屏     [标签栏 / 面板]
  // 右分屏   主列（标签栏+面板）| 分隔条 | 副列（整列到顶，头部与标签栏同高对齐）
  // 下分屏   标签栏 / 主面板 / 分隔条 / 副面板
  const gridStyle: React.CSSProperties = !effectiveSplit
    ? { gridTemplateColumns: "minmax(0, 1fr)", gridTemplateRows: "auto minmax(0, 1fr)" }
    : effectiveSplit.dir === "row"
      ? {
          gridTemplateColumns: `minmax(0, ${splitRatio}fr) 7px minmax(0, ${100 - splitRatio}fr)`,
          gridTemplateRows: "auto minmax(0, 1fr)",
        }
      : {
          gridTemplateColumns: "minmax(0, 1fr)",
          gridTemplateRows: `auto minmax(0, ${splitRatio}fr) 7px minmax(0, ${100 - splitRatio}fr)`,
        };

  return (
    <div
      ref={panesRef}
      className={
        "terminal-tabs" + (effectiveSplit ? ` is-split split-${effectiveSplit.dir}` : "")
      }
      style={gridStyle}
    >
      <div className="tab-bar" role="tablist">
        {sessions
          .filter((s) => s.id !== effectiveSplit?.id)
          .map((s) => (
            <div
              key={s.id}
              className={"tab" + (s.id === activeId ? " is-active" : "")}
              onClick={() => onActivate(s.id)}
              role="tab"
              aria-selected={s.id === activeId}
              title={`${s.projectDir}（拖到终端区可分屏）`}
              {...dragProps(s.id)}
            >
              <span className="account-dot" data-tool={s.tool} aria-hidden="true" />
              <span className="tab-label">{s.accountName}</span>
              <button
                className="tab-close"
                title="结束会话"
                onClick={(e) => {
                  e.stopPropagation();
                  if (s.id === split?.id) setSplit(null);
                  onClose(s.id);
                }}
              >
                ✕
              </button>
            </div>
          ))}
      </div>
      {sessions.map((s) => {
          const isSplitPane = s.id === effectiveSplit?.id;
          return (
            <div
              key={s.id}
              className={
                "tab-pane" +
                (s.id === activeId ? " is-active" : "") +
                (isSplitPane ? " is-split-pane" : "")
              }
            >
              {isSplitPane && splitSession && (
                <div
                  key="head"
                  className="split-pane-head"
                  title={`${splitSession.projectDir}（可拖回主屏）`}
                  {...dragProps(s.id)}
                >
                  <span
                    className="account-dot"
                    data-tool={splitSession.tool}
                    aria-hidden="true"
                  />
                  <span className="split-pane-title">{splitSession.accountName}</span>
                  <button
                    className="tab-close"
                    title="取消分屏（回到标签栏）"
                    onClick={() => setSplit(null)}
                  >
                    ◱
                  </button>
                  <button
                    className="tab-close"
                    title="结束会话"
                    onClick={() => {
                      setSplit(null);
                      onClose(s.id);
                    }}
                  >
                    ✕
                  </button>
                </div>
              )}
              <TerminalView key="term" sessionId={s.id} onExit={noopExit} />
            </div>
          );
        })}
        {effectiveSplit && (
          <div
            className={"split-divider split-divider-" + effectiveSplit.dir}
            role="separator"
            aria-orientation={effectiveSplit.dir === "row" ? "vertical" : "horizontal"}
            title="拖动调整分屏比例"
            onPointerDown={handleDividerPointerDown}
          />
        )}
        {draggingId && (
          <div className="drop-zones">
            <div
              className={"drop-zone drop-zone-main" + (dropHint === "main" ? " is-hover" : "")}
              {...zoneProps("main")}
            >
              <span className="drop-zone-label">主屏</span>
            </div>
            <div
              className={"drop-zone drop-zone-right" + (dropHint === "right" ? " is-hover" : "")}
              {...zoneProps("right")}
            >
              <span className="drop-zone-label">右分屏</span>
            </div>
            <div
              className={"drop-zone drop-zone-bottom" + (dropHint === "bottom" ? " is-hover" : "")}
              {...zoneProps("bottom")}
            >
              <span className="drop-zone-label">下分屏</span>
            </div>
          </div>
        )}
    </div>
  );
}

export default TerminalTabs;
