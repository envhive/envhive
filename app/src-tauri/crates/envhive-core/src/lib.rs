//! 蜂巢 EnvHive 共享地基（envhive-core）
//!
//! 零 Tauri / 零业务依赖的基础层，供桌面应用（envhive）与 CLI 复用：
//! - `error`：统一错误模型（EnvHiveError / EnvHiveErrorKind / bail! 宏）
//! - `config`：全局配置 `~/.envhive/config.yaml`
//! - `pathmeta`：路径元数据与目录初始化
//! - `util`：通用小工具（路径展开 / 压缩格式识别 / 原子目录替换等）
//! - `toml_chain`：三作用域配置链（Global / Project / Session）
//! - `logging`：tracing 日志初始化
//! - `env`：环境变量注入模型（Envs / 跨平台符号链接 / Windows 注册表 PATH 同步）
//! - `events`：事件负载数据类型（下载进度 / 安装状态 / 版本切换，零 Tauri 依赖）

pub mod config;
pub mod env;
pub mod error;
pub mod events;
pub mod logging;
pub mod pathmeta;
pub mod toml_chain;
pub mod util;

pub use config::AppConfig;
pub use env::Envs;
pub use error::{EnvHiveError, EnvHiveErrorKind};
pub use events::{DownloadProgress, DownloadStage};
pub use pathmeta::PathMeta;
pub use toml_chain::{ConfigChain, ScopeConfig, ToolValue};
