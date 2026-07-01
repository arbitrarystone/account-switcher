import { useEffect, useRef, useState } from "react";

import { errorMessage } from "../../lib/api";
import {
  updaterApi,
  type DownloadProgress,
  type Update,
} from "../../lib/updater";

interface UpdateCheckerProps {
  /** 当前运行版本（来自 app_version），用于对话框展示。 */
  currentVersion: string;
}

type CheckState = "idle" | "checking" | "uptodate" | "error";

/** 字节 → 友好体积。 */
function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

/**
 * 状态栏「检查更新」入口 + 更新对话框。
 *
 * - 挂载时静默检查（失败/无更新不打扰）；发现新版自动弹对话框。
 * - 手动按钮：无更新提示「已是最新」，端点不可达给出可读错误。
 * - 发现更新：展示版本 + release notes，一键下载安装并重启。
 */
function UpdateChecker({ currentVersion }: UpdateCheckerProps) {
  const [update, setUpdate] = useState<Update | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [checkState, setCheckState] = useState<CheckState>("idle");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [installing, setInstalling] = useState(false);
  const didAutoCheck = useRef(false);

  const runCheck = async (manual: boolean) => {
    if (manual) {
      setCheckState("checking");
      setErrorMsg(null);
    }
    try {
      const res = await updaterApi.check();
      if (res) {
        setUpdate(res);
        setDialogOpen(true);
        if (manual) setCheckState("idle");
      } else if (manual) {
        setCheckState("uptodate");
      }
    } catch (e: unknown) {
      // 静默检查（首个 release 前端点 404 属正常）不打扰；手动才提示。
      if (manual) {
        setCheckState("error");
        setErrorMsg(errorMessage(e));
      }
    }
  };

  // 启动静默检查，仅一次。
  useEffect(() => {
    if (didAutoCheck.current) return;
    didAutoCheck.current = true;
    void runCheck(false);
  }, []);

  // 「已是最新 / 检查失败」内联提示几秒后自动消隐。
  useEffect(() => {
    if (checkState !== "uptodate" && checkState !== "error") return;
    const t = setTimeout(() => {
      setCheckState("idle");
      setErrorMsg(null);
    }, 3500);
    return () => clearTimeout(t);
  }, [checkState]);

  const handleInstall = async () => {
    if (!update) return;
    setInstalling(true);
    setErrorMsg(null);
    setProgress({ downloaded: 0, total: null });
    try {
      await updaterApi.downloadAndInstall(update, setProgress);
      await updaterApi.relaunch();
    } catch (e: unknown) {
      setErrorMsg(errorMessage(e));
      setInstalling(false);
    }
  };

  const pct =
    progress && progress.total
      ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
      : null;

  return (
    <>
      {update && !dialogOpen ? (
        <button
          className="update-pill"
          onClick={() => setDialogOpen(true)}
          title={`发现新版本 v${update.version}`}
        >
          <span className="update-pill-dot" aria-hidden="true" />
          有新版 v{update.version}
        </button>
      ) : (
        <button
          className="update-check-btn"
          onClick={() => void runCheck(true)}
          disabled={checkState === "checking"}
        >
          {checkState === "checking"
            ? "检查中…"
            : checkState === "uptodate"
              ? "已是最新"
              : checkState === "error"
                ? "检查失败"
                : "检查更新"}
        </button>
      )}
      {checkState === "error" && errorMsg && (
        <span className="update-inline-error" title={errorMsg}>
          {errorMsg}
        </span>
      )}

      {dialogOpen && update && (
        <div
          className="dialog-backdrop"
          onClick={() => {
            if (!installing) setDialogOpen(false);
          }}
        >
          <div className="dialog dialog-sm" onClick={(e) => e.stopPropagation()}>
            <div className="dialog-head">
              <h2 className="dialog-title">发现新版本</h2>
            </div>
            <div className="dialog-body">
              <p className="update-versions">
                <span className="update-ver-old">v{currentVersion}</span>
                <span className="update-ver-arrow" aria-hidden="true">
                  →
                </span>
                <span className="update-ver-new">v{update.version}</span>
              </p>
              {update.body && (
                <pre className="update-notes">{update.body.trim()}</pre>
              )}
              {installing && (
                <div className="update-progress" aria-live="polite">
                  <div className="update-progress-track">
                    <div
                      className="update-progress-bar"
                      style={{
                        transform: `scaleX(${pct != null ? pct / 100 : 0.15})`,
                      }}
                      data-indeterminate={pct == null ? "true" : undefined}
                    />
                  </div>
                  <span className="update-progress-text">
                    {pct != null
                      ? `下载中 ${pct}%`
                      : progress
                        ? `下载中 ${formatBytes(progress.downloaded)}`
                        : "准备中…"}
                  </span>
                </div>
              )}
              {errorMsg && <p className="form-warning">⚠️ {errorMsg}</p>}
            </div>
            <div className="dialog-foot">
              <button
                className="btn btn-ghost"
                onClick={() => setDialogOpen(false)}
                disabled={installing}
              >
                稍后
              </button>
              <button
                className="btn btn-primary"
                onClick={() => void handleInstall()}
                disabled={installing}
              >
                {installing ? "安装中…" : "下载并安装"}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

export default UpdateChecker;
