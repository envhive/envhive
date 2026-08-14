//! 插件信息转换（前端展示，统一为 plugin::PluginInfo）

use envhive_core::pathmeta::PathMeta;
use crate::tool::ToolDescriptor;

use super::{plugin_file, LuaPluginDef};

pub fn to_plugin_info(paths: &PathMeta, def: &LuaPluginDef) -> crate::plugin::PluginInfo {
    // 图标：插件目录文件（svg>png>jpg）优先，其次插件声明 base64（TOOL.icon_base64）
    let icon = crate::tool::icon::resolve_icon(
        &crate::plugin::plugin_dir(paths, &def.name),
        def.icon_base64.as_deref(),
    );
    crate::plugin::PluginInfo {
        name: def.name.clone(),
        display: def.display.clone(),
        category: def.category.clone(),
        homepage: def.homepage.clone(),
        provider: "lua".into(),
        version: def.version.clone(),
        installed_versions: crate::tool::install::installed_versions(paths, &def.name),
        source: "local".into(), // 由 manager 层按实际来源覆盖
        distributions: def.distributions.clone(),
        path: crate::plugin::plugin_dir(paths, &def.name).to_string_lossy().to_string(),
        updated_at: crate::plugin::file_mtime(&plugin_file(paths, &def.name)),
        enabled: true, // 由 manager 层覆盖
        icon,
    }
}

/// 供 ToolDescriptor::lua_def 使用的 downcast 辅助
pub fn as_lua(desc: &dyn ToolDescriptor) -> Option<&LuaPluginDef> {
    desc.lua_def()
}
