//! EnvHiveManager · 插件管理（Lua）：列表 / 禁用启用 / 删除 / 源码读写

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use envhive_toolkit::plugin;

use super::EnvHiveManager;

impl EnvHiveManager {
    /// 插件列表（含禁用插件，enabled=false 表示目录为 <name>.disabled）
    pub fn list_plugins(&self) -> Vec<plugin::PluginInfo> {
        let mut out = Vec::new();
        for (name, enabled) in plugin::list_all_plugins(&self.paths) {
            let raw = if enabled {
                name.clone()
            } else {
                format!("{name}{}", plugin::DISABLED_SUFFIX)
            };
            let mut info = if let Ok(Some(lua)) = envhive_toolkit::lua_plugin::load_lua_plugin_entry(&self.paths, &raw, &name) {
                envhive_toolkit::lua_plugin::to_plugin_info(&self.paths, &lua)
            } else {
                continue;
            };
            info.source = plugin::plugin_source(&self.paths, &raw).to_string();
            info.enabled = enabled;
            // 图标按原始目录（含 <name>.disabled 禁用目录）重新解析：禁用目录内 icon 文件也生效
            info.icon = envhive_toolkit::tool::icon::resolve_icon(&self.paths.plugins.join(&raw), info.icon.as_deref());
            out.push(info);
        }
        out
    }

    /// 禁用 / 启用插件：目录重命名 `<name>` ↔ `<name>.disabled`
    ///
    /// 禁用前检查全局配置：若该 工具 仍在全局 `.envhive.toml` 中引用（已激活），
    /// 拒绝禁用并提示先取消全局设置，避免环境变量静默失效。
    pub fn toggle_plugin(&self, name: &str, enable: bool) -> Result<()> {
        if !envhive_core::util::safe_component(name) {
            return Err(EnvHiveError::new(EnvHiveErrorKind::Config, format!("非法插件名: {name:?}")));
        }
        let enabled_dir = self.paths.plugins.join(name);
        let disabled_dir = self.paths.plugins.join(format!("{name}{}", plugin::DISABLED_SUFFIX));
        let (from, to) = if enable {
            (disabled_dir, enabled_dir)
        } else {
            (enabled_dir, disabled_dir)
        };
        if !from.exists() {
            let state = if enable { "禁用" } else { "启用" };
            return Err(EnvHiveError::new(
                EnvHiveErrorKind::ToolNotFound,
                format!("插件 {name} 当前不是{state}状态"),
            ));
        }
        if !enable {
            if let Ok(chain) = self.chain(None) {
                if chain.tool(name).is_some() {
                    return Err(EnvHiveError::new(
                        EnvHiveErrorKind::Config,
                        format!("{name} 仍在全局配置中启用（已设置版本），请先在「首页」取消其全局设置后再禁用插件"),
                    ));
                }
            }
        }
        std::fs::rename(&from, &to).map_err(|e| {
            EnvHiveError::with_source(
                EnvHiveErrorKind::Io,
                format!("{}插件 {name} 失败: {}", if enable { "启用" } else { "禁用" }, from.display()),
                e,
            )
        })?;
        self.open_tools.lock().unwrap().remove(&name.to_ascii_lowercase());
        tracing::info!("插件 {name} 已{}（{} → {}）", if enable { "启用" } else { "禁用" }, from.display(), to.display());
        Ok(())
    }

    /// 删除插件（仅删除插件定义目录，含禁用变体；已安装版本目录保留，可在统计页清理）
    pub fn delete_plugin(&self, name: &str) -> Result<()> {
        if !envhive_core::util::safe_component(name) {
            return Err(EnvHiveError::new(EnvHiveErrorKind::Config, format!("非法插件名: {name:?}")));
        }
        if let Ok(chain) = self.chain(None) {
            if chain.tool(name).is_some() {
                return Err(EnvHiveError::new(
                    EnvHiveErrorKind::Config,
                    format!("{name} 仍在全局配置中启用，请先取消其全局设置后再删除插件"),
                ));
            }
        }
        let dir = self.paths.plugins.join(name);
        let disabled_dir = self.paths.plugins.join(format!("{name}{}", plugin::DISABLED_SUFFIX));
        let mut removed = false;
        for d in [&dir, &disabled_dir] {
            if d.exists() {
                std::fs::remove_dir_all(d).map_err(|e| {
                    EnvHiveError::with_source(EnvHiveErrorKind::Io, format!("删除插件失败: {}", d.display()), e)
                })?;
                removed = true;
            }
        }
        if !removed {
            return Err(EnvHiveError::new(EnvHiveErrorKind::ToolNotFound, format!("插件 {name} 不存在")));
        }
        self.open_tools.lock().unwrap().remove(&name.to_ascii_lowercase());
        tracing::info!("已删除插件 {name}（已安装版本保留）");
        Ok(())
    }

    /// 在系统文件管理器中打开插件目录（禁用目录也能打开）
    pub fn open_plugin_dir(&self, name: &str) -> Result<()> {
        let dir = self.paths.plugins.join(name);
        let dir = if dir.exists() {
            dir
        } else {
            self.paths.plugins.join(format!("{name}{}", plugin::DISABLED_SUFFIX))
        };
        if !dir.exists() {
            return Err(EnvHiveError::new(EnvHiveErrorKind::ToolNotFound, format!("插件 {name} 不存在")));
        }
        open_in_file_manager(&dir)?;
        Ok(())
    }

    /// 读取插件源码（plugin.lua；供编辑器加载）
    pub fn read_plugin_source(&self, name: &str) -> Result<envhive_toolkit::plugin::PluginSource> {
        let raw = if plugin::plugin_dir_exists(&self.paths, name) {
            name.to_string()
        } else {
            format!("{name}{}", plugin::DISABLED_SUFFIX)
        };
        let dir = self.paths.plugins.join(&raw);
        let lua_file = dir.join("plugin.lua");
        if !lua_file.exists() {
            return Err(EnvHiveError::new(EnvHiveErrorKind::ToolNotFound, format!("插件 {name} 不存在")));
        }
        let script = std::fs::read_to_string(&lua_file).map_err(|e| {
            EnvHiveError::with_source(EnvHiveErrorKind::Io, format!("读取插件失败: {}", lua_file.display()), e)
        })?;
        Ok(envhive_toolkit::plugin::PluginSource {
            name: name.to_string(),
            provider: "lua".to_string(),
            script,
        })
    }

    /// 保存插件源码：Lua 走 update（校验 + 原子写回）
    pub fn save_plugin_source(&self, name: &str, provider: &str, script: &str) -> Result<()> {
        if provider != "lua" {
            return Err(EnvHiveError::new(
                EnvHiveErrorKind::Config,
                format!("不支持的插件类型 {provider:?}（仅支持 lua）"),
            ));
        }
        envhive_toolkit::lua_plugin::update_lua_plugin(&self.paths, name, script)
    }

    /// 添加 Lua 插件（完整脚本能力）
    pub fn add_lua_sdk(&self, name: &str, script: &str) -> Result<()> {
        envhive_toolkit::lua_plugin::add_lua_plugin(&self.paths, name, script)
    }
}

/// 跨平台在系统文件管理器中打开目录（explorer / open / xdg-open）
fn open_in_file_manager(dir: &std::path::Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(dir)
            .spawn()
            .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Io, format!("打开目录失败: {}", dir.display()), e))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(dir)
            .spawn()
            .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Io, format!("打开目录失败: {}", dir.display()), e))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Io, format!("打开目录失败: {}", dir.display()), e))?;
    }
    Ok(())
}
