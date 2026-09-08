//! 桌面端事件接收器：把 `ManagerEvent` 分发为 Tauri 前端事件。
//!
//! manager 在 `tauri::Builder` 之前构造（此时尚无 `AppHandle`），因此 `AppHandle`
//! 延迟绑定：`setup` 阶段调用 `bind()` 后，安装/切换/队列事件开始推送前端。
//! 事件名与改造前完全一致（`download-progress` / `install-status` / `app-error` /
//! `version-changed` / `queue-updated`），前端零改动。

use std::sync::Mutex;

use tauri::{AppHandle, Emitter};

use envhive_manager::events::{EventSink, ManagerEvent};

pub struct TauriSink {
    app: Mutex<Option<AppHandle>>,
}

impl TauriSink {
    pub fn new() -> Self {
        TauriSink { app: Mutex::new(None) }
    }

    /// setup 阶段绑定 AppHandle（此后事件开始向前端推送）
    pub fn bind(&self, app: AppHandle) {
        *self.app.lock().unwrap() = Some(app);
    }
}

impl EventSink for TauriSink {
    fn emit(&self, evt: ManagerEvent) {
        let Some(app) = self.app.lock().unwrap().as_ref() else { return };
        match evt {
            ManagerEvent::Progress(p) => {
                let _ = app.emit("download-progress", p);
            }
            ManagerEvent::InstallStatus(s) => {
                let _ = app.emit("install-status", s);
            }
            ManagerEvent::Error { tool, code, message } => {
                let payload = serde_json::json!({ "tool": tool, "code": code, "message": message });
                let _ = app.emit("app-error", payload);
                tracing::warn!("[{}] {} {}", tool, code, message);
            }
            ManagerEvent::VersionChanged(v) => {
                let _ = app.emit("version-changed", v);
            }
            ManagerEvent::QueueUpdated(tasks) => {
                let _ = app.emit("queue-updated", tasks);
            }
        }
    }
}
