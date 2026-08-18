//! 蜂巢 EnvHive 工具与插件域（envhive-toolkit）
//!
//! 零 Tauri 依赖，供编排层（manager）、UI 层（commands）与 CLI（envhive-cli）复用：
//! - `tool`：Tool/ToolDescriptor trait、版本解析、下载、安装、校验、图标
//! - `plugin`：插件域共享基础设施（插件目录 / 来源 / 列表 / 展示信息）
//! - `lua_plugin`：Lua 脚本插件（LuaPluginDef / LuaTool / VM hooks）
//! - `registry`：镜像源与仓库配置管理（npm / pip / cargo / maven / go / docker / nuget / gem / pub / conda）
//! - `mirror`：下载加速镜像（对官方下载 URL 做前缀替换）
//! - `env_resolver`：环境解析器（配置链 → 合并 Envs，CLI 与桌面共用）
//! - `shell`：Shell 脚本生成（bash/zsh/fish/powershell/cmd 环境注入）

pub mod env_resolver;
pub mod lua_plugin;
pub mod mirror;
pub mod plugin;
pub mod registry;
pub mod shell;
pub mod tool;

pub use env_resolver::{resolve_envs, ToolLookup};
pub use lua_plugin::{add_lua_plugin, LuaPluginDef, LuaTool};
pub use mirror::{apply as apply_mirror, BUILTIN_RULES};
pub use registry::{RegistryManager, RegistryPreset, RegistryState, ToolRegistryWriter};
pub use shell::{render as render_env_script, ShellKind};
pub use tool::version::Version;
pub use tool::{Tool, ToolDescriptor, ToolInfo, ToolMap};
