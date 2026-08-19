//! 事件抽象（envhive-manager）—— 零 Tauri 依赖
//!
//! 事件负载定义在 envhive-core（`envhive_core::events`，纯数据）。本模块定义统一的
//! 事件分发抽象 `EventSink`：桌面端注入 Tauri 实现（TauriSink，位于 app 壳层），
//! CLI/TUI 注入 channel 实现（ChannelSink），无 UI 场景使用 `NullSink`。
//! 长任务（安装 / 队列 / 切换）不再持有 `AppHandle`。

pub use envhive_core::events::{DownloadProgress, DownloadStage, InstallStatus, VersionChanged};

use crate::queue::QueueTask;

/// 统一事件（manager 层对外事件总线）
#[derive(Debug, Clone)]
pub enum ManagerEvent {
    /// 下载进度（前端事件名 `download-progress`）
    Progress(DownloadProgress),
    /// 安装状态（前端事件名 `install-status`）
    InstallStatus(InstallStatus),
    /// 错误（前端事件名 `app-error`）
    Error { tool: String, code: String, message: String },
    /// 版本切换成功（前端事件名 `version-changed`）
    VersionChanged(VersionChanged),
    /// 队列快照更新（前端事件名 `queue-updated`）
    QueueUpdated(Vec<QueueTask>),
}

/// 事件接收器：桌面注入 Tauri 实现，CLI/TUI 注入 channel 实现，无 UI 时用 NullSink
pub trait EventSink: Send + Sync {
    fn emit(&self, evt: ManagerEvent);
}

/// 空实现：丢弃全部事件（CLI 非交互场景 / 测试）
#[derive(Default)]
pub struct NullSink;
impl EventSink for NullSink {
    fn emit(&self, _evt: ManagerEvent) {}
}
