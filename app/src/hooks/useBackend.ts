// useBackend：invoke 封装 + Tauri 事件订阅（Vue 3 composable 版本）
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DownloadProgress, QueueTask } from "../types";

// 统一命令调用：成功返回数据；预览模式（非 Tauri 环境）下抛错由调用方捕获
export async function run<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args);
}

// 系统通知（WebView2 Notification API；失败静默回退）
export async function notify(title: string, body: string) {
  try {
    if (typeof Notification === "undefined") return;
    if (Notification.permission === "granted") {
      new Notification(title, { body });
    } else if (Notification.permission !== "denied") {
      const perm = await Notification.requestPermission();
      if (perm === "granted") new Notification(title, { body });
    }
  } catch {
    // 预览模式 / 无权限：忽略
  }
}

export interface EventsOpts {
  onProgress: (p: DownloadProgress) => void;
  onQueue: (q: QueueTask[]) => void;
  onDone: (p: DownloadProgress) => void;
  onError: (message: string) => void;
}

// 事件订阅（App 挂载时注册一次，返回清理函数）
export function setupEvents(opts: EventsOpts): () => Promise<() => void> {
  return async () => {
    const fns: UnlistenFn[] = [];
    const safe = async (p: Promise<UnlistenFn>) => {
      try {
        fns.push(await p);
      } catch {
        // 预览模式忽略
      }
    };

    await safe(
      listen<DownloadProgress>("download-progress", (e) => {
        opts.onProgress(e.payload);
        if (e.payload.stage === "done") opts.onDone(e.payload);
        if (e.payload.stage === "failed")
          opts.onError(`${e.payload.tool} ${e.payload.version} 下载失败`);
      })
    );
    await safe(listen<QueueTask[]>("queue-updated", (e) => opts.onQueue(e.payload)));
    await safe(
      listen<{ tool: string; code: string; message: string }>("app-error", (e) =>
        opts.onError(e.payload.message)
      )
    );
    return () => fns.forEach((f) => f());
  };
}
