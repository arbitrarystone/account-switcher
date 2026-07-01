/**
 * 应用内检查更新封装（tauri-plugin-updater + tauri-plugin-process）。
 *
 * 更新产物由 GitHub Releases 提供（见 .github/workflows/release.yml），
 * 端点在 tauri.conf.json 的 plugins.updater.endpoints 配置。
 */
import {
  check,
  type DownloadEvent,
  type Update,
} from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type { Update };

/** 下载进度（total 为 null 表示服务器未返回长度，进度不确定）。 */
export interface DownloadProgress {
  downloaded: number;
  total: number | null;
}

export const updaterApi = {
  /** 查询是否有新版本；无更新返回 null。端点不可达 / 未发版会抛错，由调用方兜底。 */
  check: (): Promise<Update | null> => check(),

  /** 下载并安装更新，全程回调进度。安装完成不自动重启（由调用方决定时机）。 */
  downloadAndInstall: async (
    update: Update,
    onProgress?: (p: DownloadProgress) => void,
  ): Promise<void> => {
    let downloaded = 0;
    let total: number | null = null;
    await update.downloadAndInstall((event: DownloadEvent) => {
      switch (event.event) {
        case "Started":
          total = event.data.contentLength ?? null;
          break;
        case "Progress":
          downloaded += event.data.chunkLength;
          break;
        case "Finished":
          break;
      }
      onProgress?.({ downloaded, total });
    });
  },

  /** 安装完成后重启应用以加载新版本。 */
  relaunch: (): Promise<void> => relaunch(),
};
