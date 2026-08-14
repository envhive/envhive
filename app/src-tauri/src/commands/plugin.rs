//! 插件域命令：插件管理（Lua）/ 环境预览 / 进程注入 / 远程插件

use serde::Serialize;
use tauri::State;

use crate::error::{EnvHiveError, EnvHiveErrorKind, Result};

use super::AppState;

/// 已安装插件（自定义 工具）列表
#[tauri::command]
pub fn list_plugins(state: State<'_, AppState>) -> Result<Vec<crate::plugin::PluginInfo>> {
    Ok(state.manager.list_plugins())
}

/// 环境预览（Session 落地：暂存 Envs 不落盘，diff 展示）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvPreview {
    pub path_entries: Vec<String>,
    pub vars: Vec<EnvDiff>,
    pub export_script: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvDiff {
    pub key: String,
    pub value: Option<String>, // None = unset
    pub source: String,
}

#[tauri::command]
pub fn preview_env(state: State<'_, AppState>) -> Result<EnvPreview> {
    let envs = state.manager.resolve_envs(None)?;
    let mut vars: Vec<EnvDiff> = envs
        .vars
        .iter()
        .map(|(k, v)| EnvDiff { key: k.clone(), value: v.clone(), source: "global".into() })
        .collect();
    vars.sort_by(|a, b| a.key.cmp(&b.key));
    // 导出脚本（bash）
    let mut lines: Vec<String> = Vec::new();
    if !envs.paths.is_empty() {
        let path_prefix = envs.paths.join(":");
        lines.push(format!("export PATH=\"{path_prefix}:${{PATH:+$PATH}}\""));
    }
    for v in &vars {
        match &v.value {
            Some(val) => lines.push(format!("export {}={:?}", v.key, val)),
            None => lines.push(format!("unset {}", v.key)),
        }
    }
    Ok(EnvPreview {
        path_entries: envs.paths.clone(),
        vars,
        export_script: lines.join("\n"),
    })
}

/// 以此环境启动子进程（进程注入 Session）
#[tauri::command]
pub fn spawn_with_env(state: State<'_, AppState>, command: String, args: Vec<String>) -> Result<()> {
    state.manager.spawn_with_env(None, &command, &args)
}

/// 远程注册表：可用插件列表（address 缺省时用第一个已配置地址）
#[tauri::command]
pub async fn list_remote_plugins(
    state: State<'_, AppState>,
    address: Option<String>,
) -> Result<Vec<crate::extras::RemotePluginInfo>> {
    let addresses = state.manager.config.lock().unwrap().registry_addresses();
    let target = address
        .as_deref()
        .map(|a| a.trim().trim_end_matches('/').to_string())
        .filter(|a| !a.is_empty())
        .or_else(|| addresses.first().cloned())
        .ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Config, "未配置远程插件仓库地址"))?;
    if !target.starts_with("https://") && !target.starts_with("http://") {
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::Config,
            "插件仓库地址需以 http:// 或 https:// 开头",
        ));
    }
    let client = state.manager.client();
    crate::extras::fetch_remote_manifest(&target, &client).await
}

/// 安装远程插件（整个 manifest 条目传入：type/format/sha256 随包校验，支持 zip / 直链）
#[tauri::command]
pub async fn install_remote_plugin(
    state: State<'_, AppState>,
    plugin: crate::extras::RemotePluginInfo,
) -> Result<()> {
    let client = state.manager.client();
    crate::extras::install_remote_plugin(&state.manager.paths, &plugin, &client).await
}

/// 添加 Lua 插件（完整脚本能力：available / pre_install / env_keys / pre_uninstall hook）
#[tauri::command]
pub fn add_lua_plugin(state: State<'_, AppState>, name: String, script: String) -> Result<()> {
    state.manager.add_lua_sdk(&name, &script)
}

/// 禁用 / 启用插件（目录重命名 `<name>` ↔ `<name>.disabled`；禁用前检查全局引用）
#[tauri::command]
pub fn toggle_plugin(state: State<'_, AppState>, name: String, enable: bool) -> Result<()> {
    state.manager.toggle_plugin(&name, enable)
}

/// 删除插件（仅删插件定义目录，已安装版本保留）
#[tauri::command]
pub fn delete_plugin(state: State<'_, AppState>, name: String) -> Result<()> {
    state.manager.delete_plugin(&name)
}

/// 在系统文件管理器中打开插件目录
#[tauri::command]
pub fn open_plugin_dir(state: State<'_, AppState>, name: String) -> Result<()> {
    state.manager.open_plugin_dir(&name)
}

/// 读取插件源码（编辑器加载；lua）
#[tauri::command]
pub fn read_plugin_source(state: State<'_, AppState>, name: String) -> Result<crate::plugin::PluginSource> {
    state.manager.read_plugin_source(&name)
}

/// 保存插件源码（Lua：校验 + 原子写回）
#[tauri::command]
pub fn save_plugin_source(
    state: State<'_, AppState>,
    name: String,
    provider: String,
    script: String,
) -> Result<()> {
    state.manager.save_plugin_source(&name, &provider, &script)
}
