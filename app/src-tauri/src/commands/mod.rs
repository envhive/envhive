//! Tauri Command 层（roadmap A：所有接口统一返回 `Result<T, EnvHiveError>`）
//! 前端 UI 直接消费的命令。
//!
//! 按业务域拆分子模块（`mod.rs` 仅保留 `AppState` 与重导出）：
//! - `tool`：工具 列表 / 版本 / 安装 / 卸载 / 全局切换 / 下载队列
//! - `config`：全局配置 / 日志 / 目录 / 代理
//! - `registry`：镜像源预设 / 下载加速镜像
//! - `plugin`：插件（Lua）/ 环境预览 / 远程插件
//! - `system`：自启动 / 托盘 / 统计 / 冲突 / 导入导出 / 系统环境变量
//! - `projects`：项目预设与会话启动
//! - `home`：首页总览

mod config;
mod home;
mod plugin;
mod projects;
mod registry;
mod tool;
mod system;

pub use config::*;
pub use home::*;
pub use plugin::*;
pub use projects::*;
pub use registry::*;
pub use tool::*;
pub use system::*;

use std::sync::Arc;

use crate::manager::EnvHiveManager;
use crate::queue::QueueManager;

/// 应用全局状态（由 lib.rs manage）
pub struct AppState {
    pub manager: Arc<EnvHiveManager>,
    pub queue: Arc<QueueManager>,
}
