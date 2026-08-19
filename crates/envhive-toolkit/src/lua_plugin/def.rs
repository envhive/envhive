//! Lua 插件定义解析与文件管理：加载 / 校验 / 新增 / 更新 / 默认插件注入

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mlua::{Function, Table};

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use envhive_core::pathmeta::PathMeta;
use crate::tool::DistributionInfo;
use envhive_core::util::safe_component;

use super::LuaPluginDef;
use super::vm::new_plugin_lua;

/// 插件文件路径（`plugins/<name>/plugin.lua`）
pub fn plugin_file(paths: &PathMeta, name: &str) -> PathBuf {
    paths.plugins.join(name).join("plugin.lua")
}

/// 是否 Lua 插件目录（存在 plugin.lua）
pub fn is_lua_plugin(paths: &PathMeta, name: &str) -> bool {
    plugin_file(paths, name).exists()
}

/// 加载 Lua 插件（不存在返回 None）。
/// 加载即校验：语法 + 工具 元信息 + 必需 hook（available / pre_install）。
/// 失败返回 Err 并记录详细日志 —— 坏插件在此被拦截，不会带到运行时。
pub fn load_lua_plugin(paths: &PathMeta, name: &str) -> Result<Option<LuaPluginDef>> {
    let file = plugin_file(paths, name);
    if !file.exists() {
        return Ok(None);
    }
    let script = std::fs::read_to_string(&file).map_err(|e| {
        EnvHiveError::with_source(EnvHiveErrorKind::Io, format!("读取插件失败: {}", file.display()), e)
    })?;
    match build_plugin_def(name, &script, &paths.root) {
        Ok(def) => {
            tracing::info!("[{}] Lua 插件加载成功（verify_bin={}）", def.name, def.verify_bin);
            Ok(Some(def))
        }
        Err(e) => {
            tracing::error!("[{}] Lua 插件加载失败（{}）: {}", name, file.display(), e);
            Err(e)
        }
    }
}

/// 按原始目录名加载 Lua 插件（兼容 `<name>.disabled` 禁用目录；name 为真实插件名）。
/// 与 `load_lua_plugin` 的区别仅在读文件路径 —— 禁用目录需从 `<name>.disabled/` 读取。
pub fn load_lua_plugin_entry(paths: &PathMeta, raw: &str, name: &str) -> Result<Option<LuaPluginDef>> {
    let file = paths.plugins.join(raw).join("plugin.lua");
    if !file.exists() {
        return Ok(None);
    }
    let script = std::fs::read_to_string(&file).map_err(|e| {
        EnvHiveError::with_source(EnvHiveErrorKind::Io, format!("读取插件失败: {}", file.display()), e)
    })?;
    match build_plugin_def(name, &script, &paths.root) {
        Ok(def) => {
            tracing::info!("[{}] Lua 插件加载成功（verify_bin={}）", def.name, def.verify_bin);
            Ok(Some(def))
        }
        Err(e) => {
            tracing::error!("[{}] Lua 插件加载失败（{}）: {}", name, file.display(), e);
            Err(e)
        }
    }
}

/// 更新已有 Lua 插件：先校验（语法 + 工具 元信息 + 必需 hook），通过后原子写回。
/// 与 `add_lua_plugin` 的区别：目录必须已存在（更新语义），且用临时文件 + rename 防止写坏。
pub fn update_lua_plugin(paths: &PathMeta, name: &str, script: &str) -> Result<()> {
    if !safe_component(name) {
        return Err(EnvHiveError::new(EnvHiveErrorKind::Config, format!("非法 工具 名称: {name:?}")));
    }
    let file = plugin_file(paths, name);
    if !file.exists() {
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::ToolNotFound,
            format!("Lua 插件 {name} 不存在（新建插件请用「新建插件」）"),
        ));
    }
    // 先解析校验（与 add 相同），通过才写盘
    if let Err(e) = build_plugin_def(name, script, &paths.root) {
        tracing::error!("[{}] 拒绝保存 Lua 插件: {e}", name);
        return Err(e);
    }
    let tmp = file.with_extension("lua.tmp");
    std::fs::write(&tmp, script)?;
    std::fs::rename(&tmp, &file).map_err(|e| {
        EnvHiveError::with_source(EnvHiveErrorKind::Io, format!("写回插件失败: {}", file.display()), e)
    })?;
    tracing::info!("已更新 Lua 工具 插件 {name} -> {}", file.display());
    Ok(())
}

/// 解析脚本 → 插件定义（不依赖插件目录，供 add_lua_plugin 校验复用）
/// `pub`：lua_plugin 模块内部校验 + 上层（extras 远程 zip 安装强校验）使用。
pub fn build_plugin_def(name: &str, script: &str, data_root: &Path) -> Result<LuaPluginDef> {
    let lib_dir = data_root.join("plugins").join(name).join("lib");
    let lua = new_plugin_lua(data_root, Some(&lib_dir))?;
    lua.load(script).exec().map_err(|e| {
        EnvHiveError::with_source(
            EnvHiveErrorKind::Config,
            format!("执行插件 {name} 脚本失败（语法错误）"),
            e,
        )
    })?;

    let globals = lua.globals();
    let sdk_table: Table = globals.get("TOOL").map_err(|_| {
        EnvHiveError::new(EnvHiveErrorKind::Config, format!("Lua 插件 {name} 缺少 工具 元信息表"))
    })?;
    let get_str = |k: &str| -> String {
        sdk_table.get::<String>(k).unwrap_or_default()
    };
    let verify_bin = get_str("verify_bin");
    // verify_bin 可空：空 = 安装后跳过运行验证（解压即用型 工具，如 Tomcat）。
    // 强制校验必需 hook：缺失直接拒绝，避免运行时才发现
    if globals.get::<Function>("available").is_err() {
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::Config,
            format!("Lua 插件 {name} 缺少必需的 available() hook"),
        ));
    }
    if globals.get::<Function>("pre_install").is_err() {
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::Config,
            format!("Lua 插件 {name} 缺少必需的 pre_install() hook"),
        ));
    }
    let root_hint: Option<String> = sdk_table.get("root_hint").ok().flatten();
    let env_vars: Vec<(String, String)> = sdk_table
        .get::<Table>("env_vars")
        .ok()
        .map(|t| t.pairs::<String, String>().filter_map(|p| p.ok()).collect())
        .unwrap_or_default();
    // 发行商维度（TOOL.distributions）—— 每项 { key, display, default? }
    let distributions: Vec<DistributionInfo> = sdk_table
        .get::<Table>("distributions")
        .ok()
        .map(|t| {
            t.sequence_values::<Table>()
                .filter_map(|v| v.ok())
                .filter_map(|row| {
                    let key: String = row.get("key").ok()?;
                    if key.is_empty() {
                        return None;
                    }
                    let display: String = row.get("display").ok().unwrap_or_else(|| key.clone());
                    Some(DistributionInfo { key, display })
                })
                .collect()
        })
        .unwrap_or_default();
    // 缺省发行商（TOOL.default_distribution；无则取 distributions 中 default=true 的 key）
    let default_distribution: Option<String> = sdk_table
        .get("default_distribution")
        .ok()
        .flatten()
        .or_else(|| {
            sdk_table
                .get::<Table>("distributions")
                .ok()
                .and_then(|t| {
                    t.sequence_values::<Table>()
                        .filter_map(|v| v.ok())
                        .find(|row| row.get::<bool>("default").ok().unwrap_or(false))
                        .and_then(|row| row.get::<String>("key").ok())
                })
        });
    let display = get_str("display");
    let display = if display.is_empty() { name.to_string() } else { display };
    // 插件自身版本号（TOOL.version 可选；空串视为未声明）
    let version: Option<String> = sdk_table.get::<String>("version").ok().filter(|v| !v.is_empty());

    // 下载加速镜像候选（TOOL.mirrors）—— 每项 { name, from, to }，前端可切换多地址
    let mirrors: Vec<crate::tool::MirrorCandidate> = sdk_table
        .get::<Table>("mirrors")
        .ok()
        .map(|t| {
            t.sequence_values::<Table>()
                .filter_map(|v| v.ok())
                .filter_map(|row| {
                    let name: String = row.get("name").ok()?;
                    let from: String = row.get("from").ok()?;
                    let to: String = row.get("to").ok()?;
                    if name.is_empty() || from.is_empty() || to.is_empty() {
                        return None;
                    }
                    Some(crate::tool::MirrorCandidate { name, from, to })
                })
                .collect()
        })
        .unwrap_or_default();
    // 缺省加速镜像名（TOOL.default_mirror；缺省由调用方取首个非官方候选）
    let default_mirror: Option<String> = sdk_table.get("default_mirror").ok().flatten();
    // 工具 图标声明（TOOL.icon_base64）：base64 原文或 data URI；文件 icon.svg/png/jpg 优先于声明
    let icon_base64: Option<String> = sdk_table
        .get::<String>("icon_base64")
        .ok()
        .filter(|s| !s.trim().is_empty());

    let def = LuaPluginDef {
        name: name.to_string(),
        display,
        category: get_str("category"),
        homepage: get_str("homepage"),
        version,
        verify_bin,
        verify_arg: get_str("verify_arg"),
        bin_suffix: get_str("bin_suffix"),
        root_hint,
        env_vars,
        distributions,
        default_distribution,
        mirrors,
        default_mirror,
        icon_base64,
        lib_dir,
        script: script.to_string(),
        dynamic_root_hint: Arc::new(Mutex::new(None)),
        data_root: data_root.to_path_buf(),
    };
    Ok(def)
}

/// 列出 Lua 插件名（仅启用；`<name>.disabled` 目录为禁用状态，不参与正常加载）
pub fn list_lua_plugins(paths: &PathMeta) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(&paths.plugins) else {
        return Vec::new();
    };
    rd.filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| {
            let raw = e.file_name().to_string_lossy().to_string();
            !raw.ends_with(crate::plugin::DISABLED_SUFFIX) && is_lua_plugin(paths, &raw)
        })
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect()
}

/// 添加 Lua 插件（写入 + 校验加载；失败自动回滚删除）
pub fn add_lua_plugin(paths: &PathMeta, name: &str, script: &str) -> Result<()> {
    if !safe_component(name) {
        return Err(EnvHiveError::new(EnvHiveErrorKind::Config, format!("非法 工具 名称: {name:?}")));
    }
    // 先解析校验（语法 + 工具 元信息 + 必需 hook），通过才写盘
    if let Err(e) = build_plugin_def(name, script, &paths.root) {
        tracing::error!("[{}] 拒绝安装 Lua 插件: {e}", name);
        return Err(e);
    }
    let dir = paths.plugins.join(name);
    std::fs::create_dir_all(&dir)?;
    let file = plugin_file(paths, name);
    std::fs::write(&file, script)?;
    tracing::info!("已添加 Lua 工具 插件 {name} -> {}", file.display());
    Ok(())
}
