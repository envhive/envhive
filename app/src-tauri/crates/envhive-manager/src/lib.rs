//! 蜂巢 EnvHive 编排层（envhive-manager）
//!
//! Tauri 业务编排：组合 toolkit（工具/插件/镜像）与 core（配置/路径），
//! 对外提供安装/切换/环境注入/队列/统计/插件同步等能力。
//! - `manager`：EnvHiveManager（工具 查找 / 版本 / 安装 / 切换 / 镜像 / 插件）
//! - `queue`：下载安装队列（长任务串行执行 + 取消）
//! - `usage`：使用统计（record_use / stats）
//! - `autostart`：开机自启动（Windows / macOS / Linux）
//! - `projects`：项目预设与会话启动
//! - `extras`：远程插件同步（Git 仓库 manifest + zip）
//! - `events`：Tauri 事件发射（进度 / 安装状态 / 版本切换）

pub mod autostart;
pub mod events;
pub mod extras;
pub mod manager;
pub mod projects;
pub mod queue;
pub mod usage;

pub use manager::{EnvHiveManager, ApplyGlobalResult};
pub use queue::QueueManager;
pub use usage::UsageStats;
