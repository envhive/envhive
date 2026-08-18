//! 事件推送（Tauri 层）—— 长任务通过 `AppHandle::emit` 推送给前端。
//! 事件名：`download-progress` / `install-status` / `app-error` / `version-changed`
//! 事件负载类型定义在 envhive-core（`envhive_core::events`），本模块仅负责发射。

use tauri::{AppHandle, Emitter};

pub use envhive_core::events::{DownloadProgress, DownloadStage, InstallStatus, VersionChanged};

pub fn emit_progress(app: &AppHandle, p: &DownloadProgress) {
    let _ = app.emit("download-progress", p);
    tracing::trace!("progress {}/{} {:.0}% {:?}", p.tool, p.version, p.percent, p.stage);
}

pub fn emit_install_status(app: &AppHandle, tool: &str, version: &str, stage: &str, detail: Option<String>) {
    let _ = app.emit(
        "install-status",
        InstallStatus { tool: tool.into(), version: version.into(), stage: stage.into(), detail },
    );
}

#[allow(dead_code)]
pub fn emit_error(app: &AppHandle, tool: &str, code: &str, message: &str) {
    let payload = serde_json::json!({ "tool": tool, "code": code, "message": message });
    let _ = app.emit("app-error", payload);
    tracing::warn!("[{}] {} {}", tool, code, message);
}

pub fn emit_version_changed(app: &AppHandle, tool: &str, version: &str) {
    let _ = app.emit("version-changed", VersionChanged { tool: tool.into(), version: version.into() });
}
