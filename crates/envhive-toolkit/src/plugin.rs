//! 插件域共享基础设施（插件目录 / 来源 / 列表 / 展示信息）
//!
//! 插件文件：`~/.envhive/plugins/<name>/plugin.lua`（Lua 脚本插件）

use std::path::PathBuf;

use envhive_core::pathmeta::PathMeta;

/// 禁用插件目录后缀：`plugins/<name>.disabled/`
pub const DISABLED_SUFFIX: &str = ".disabled";

/// 插件目录（真实名目录）
pub fn plugin_dir(paths: &PathMeta, name: &str) -> PathBuf {
    paths.plugins.join(name)
}

/// 插件目录是否存在（真实名目录；不含 .disabled 后缀判断）
pub fn plugin_dir_exists(paths: &PathMeta, name: &str) -> bool {
    plugin_dir(paths, name).exists()
}

/// 列出已安装插件名（仅启用；`<name>.disabled` 目录为禁用状态，不参与正常加载）。
pub fn list_plugins(paths: &PathMeta) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(&paths.plugins) else {
        return Vec::new();
    };
    rd.filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            !name.ends_with(DISABLED_SUFFIX) && crate::lua_plugin::is_lua_plugin(paths, &name)
        })
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect()
}

/// 列出全部插件（含禁用变体）：返回 `(真实插件名, 是否启用)`。
/// 已安装列表需要展示禁用插件（否则 UI 无法重新启用），故单独提供；
/// 正常加载路径（list_tools / lookup_tool）仍走 `list_plugins`（仅启用）。
pub fn list_all_plugins(paths: &PathMeta) -> Vec<(String, bool)> {
    let Ok(rd) = std::fs::read_dir(&paths.plugins) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.filter_map(|e| e.ok()) {
        if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let raw = e.file_name().to_string_lossy().to_string();
        let (name, enabled) = match raw.strip_suffix(DISABLED_SUFFIX) {
            Some(base) => (base.to_string(), false),
            None => (raw.clone(), true),
        };
        // 用原始目录名判定是否为插件目录（plugin.lua 存在）
        if paths.plugins.join(&raw).join("plugin.lua").exists() {
            out.push((name, enabled));
        }
    }
    out
}

/// 插件来源：`market`（远程 Git 仓库安装，带 .market 标记） / `local`（本地创建）。
pub fn plugin_source(paths: &PathMeta, raw: &str) -> &'static str {
    if paths.plugins.join(raw).join(".market").exists() {
        "market"
    } else {
        "local"
    }
}

/// 插件源码（编辑器加载用）
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSource {
    pub name: String,
    /// 插件类型：lua
    pub provider: String,
    pub script: String,
}

/// 插件信息（前端展示）
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub name: String,
    pub display: String,
    pub category: String,
    pub homepage: String,
    pub provider: String,
    /// 插件自身版本号（TOOL.version；未声明为 None）
    pub version: Option<String>,
    pub installed_versions: Vec<String>,
    /// 来源：market（远程插件仓库）/ local（本地创建）
    pub source: String,
    /// 发行商维度（TOOL.distributions 声明）
    pub distributions: Vec<crate::tool::DistributionInfo>,
    /// 插件目录路径（真实名目录，不含 .disabled 后缀）
    pub path: String,
    /// 插件定义文件最后修改时间（epoch 秒；读取失败为 None）
    pub updated_at: Option<i64>,
    /// 是否启用（false = 目录为 <name>.disabled）
    pub enabled: bool,
    /// 工具 图标 data URI（无图标为 None，前端回退彩色圆点）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// 文件最后修改时间（epoch 秒；失败为 None）
pub fn file_mtime(p: &std::path::Path) -> Option<i64> {
    std::fs::metadata(p)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}
