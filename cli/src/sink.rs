//! CLI 事件接收器：`ManagerEvent` → mpsc channel。
//!
//! - `ChannelSink`：原始事件投递（`install` 命令行内进度用）。
//! - `UiSink`：包装为 `UiMsg::Manager` 投递到 TUI 消息通道。

use tokio::sync::mpsc;

use envhive_manager::events::{EventSink, ManagerEvent};

/// 把 ManagerEvent 直接投递到 channel（非交互 install 进度用）
#[derive(Clone)]
pub struct ChannelSink(pub mpsc::UnboundedSender<ManagerEvent>);

impl EventSink for ChannelSink {
    fn emit(&self, evt: ManagerEvent) {
        let _ = self.0.send(evt);
    }
}

/// 把 ManagerEvent 包装为 `UiMsg::Manager` 投递到 TUI 通道
#[derive(Clone)]
pub struct UiSink(pub mpsc::UnboundedSender<crate::tui::UiMsg>);

impl EventSink for UiSink {
    fn emit(&self, evt: ManagerEvent) {
        let _ = self.0.send(crate::tui::UiMsg::Manager(evt));
    }
}
