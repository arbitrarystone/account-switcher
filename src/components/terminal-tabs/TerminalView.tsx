import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";

import type { UnlistenFn } from "@tauri-apps/api/event";
import { sessionApi } from "../../lib/api";

interface TerminalViewProps {
  sessionId: string;
  onExit?: (code: number) => void;
}

/** xterm 终端主题（hex，配合 app 的深色配色）。 */
const THEME = {
  background: "#14161b",
  foreground: "#e6e7eb",
  cursor: "#46c7d6",
  cursorAccent: "#14161b",
  selectionBackground: "#2a2e38",
  black: "#14161b",
  brightBlack: "#5a6173",
};

/** 单个 PTY 会话的终端视图：渲染输出、捕获输入、随容器 resize。 */
function TerminalView({ sessionId, onExit }: TerminalViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const term = new Terminal({
      fontFamily:
        'ui-monospace, "SF Mono", "JetBrains Mono", Menlo, Consolas, monospace',
      fontSize: 13,
      lineHeight: 1.2,
      cursorBlink: true,
      theme: THEME,
      scrollback: 5000,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(container);

    // fit + 把实际尺寸同步给 PTY。容器尺寸为 0（如标签未激活）时 fit 抛错，忽略。
    const syncSize = () => {
      try {
        fit.fit();
        void sessionApi.resize(sessionId, term.rows, term.cols);
      } catch {
        /* 容器尺寸为 0 时 fit 会抛错 */
      }
    };

    // 首次 fit 必须等等宽字体加载完成 —— 否则用 fallback 字体度量 cell 宽度，
    // 算出的 cols 偏差，导致子进程按错误宽度换行（格式错位）。
    let raf = 0;
    const initialFit = () => {
      raf = requestAnimationFrame(syncSize);
    };
    if (document.fonts && document.fonts.status !== "loaded") {
      void document.fonts.ready.then(initialFit);
    } else {
      initialFit();
    }

    // 输入：键入 → 写回 PTY
    const dataDisposable = term.onData((data) => {
      void sessionApi.write(sessionId, data);
    });

    // 输出 / 退出订阅（异步注册，注意卸载竞态）
    let disposed = false;
    const unsubs: UnlistenFn[] = [];
    const track = (p: Promise<UnlistenFn>) => {
      void p.then((u) => (disposed ? u() : unsubs.push(u)));
    };
    track(
      sessionApi.onOutput((o) => {
        if (o.sessionId === sessionId) term.write(o.data);
      }),
    );
    track(
      sessionApi.onExit((e) => {
        if (e.sessionId === sessionId) {
          term.write(`\r\n\x1b[2m[进程已退出，代码 ${e.code}]\x1b[0m\r\n`);
          onExit?.(e.code);
        }
      }),
    );

    // 容器尺寸变化（含标签激活、窗口/侧栏变化）→ refit + 同步 PTY
    const observer = new ResizeObserver(() => syncSize());
    observer.observe(container);
    term.focus();

    return () => {
      disposed = true;
      cancelAnimationFrame(raf);
      dataDisposable.dispose();
      unsubs.forEach((u) => u());
      observer.disconnect();
      term.dispose();
    };
  }, [sessionId, onExit]);

  return <div className="terminal-view" ref={containerRef} />;
}

export default TerminalView;
