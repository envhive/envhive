// updater.ts —— 应用内更新封装
// 依赖 @tauri-apps/plugin-updater + @tauri-apps/plugin-process（Rust 端已注册，capabilities 已授权）
// 更新清单（latest.json）与签名配置在 src-tauri/tauri.conf.json → plugins.updater

import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/** check() 返回的 Update 类型（不依赖插件类型导出，避免版本漂移） */
type Update = NonNullable<Awaited<ReturnType<typeof check>>>;

/**
 * 检查是否有可用更新。
 * @returns Update 对象（有更新）| null（已是最新）| undefined（检查失败：未配置 / 网络不可达 / 非打包环境）
 */
export async function checkForUpdates(): Promise<Update | null | undefined> {
  try {
    return await check();
  } catch (err) {
    // 常见于：endpoints 未配置、清单地址不可达、签名校验失败、dev 模式下未启用
    console.warn("[updater] 检查更新失败:", err);
    return undefined;
  }
}

/**
 * 下载并安装更新，完成后自动重启应用使新版本生效。
 * @param update check() 返回的 Update 对象
 * @param onProgress 下载进度回调（0 ~ 1）
 */
export async function downloadAndInstall(
  update: Update,
  onProgress?: (percent: number) => void,
): Promise<void> {
  let total = 0; // 总大小：Started 事件携带 contentLength
  let downloaded = 0; // 累计下载量：Progress 事件每次给出 chunkLength 增量
  await update.downloadAndInstall((event) => {
    switch (event.event) {
      case "Started":
        total = event.data.contentLength ?? 0;
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        onProgress?.(total > 0 ? downloaded / total : 0);
        break;
      case "Finished":
        break;
    }
  });
  // 安装完成后重启（Windows 上 updater 会在安装前自动退出应用）
  await relaunch();
}
