//! 镜像源域命令：预设列表 / 自定义源 / 状态 / 应用 / 下载加速镜像

use tauri::State;

use crate::error::Result;
use crate::registry::{PresetInfo, RegistryState};

use super::AppState;

/// 某工具的镜像预设列表（内置 + 用户自定义）
#[tauri::command]
pub fn list_registry_presets(state: State<'_, AppState>, tool: String) -> Result<Vec<PresetInfo>> {
    state.manager.registry.writer(&tool)?;
    let customs = state.manager.config.lock().unwrap().custom_registry.clone();
    Ok(crate::registry::presets_info(&tool, &customs))
}

/// 添加用户自定义镜像源（持久化到 config.yaml；同名覆盖；与内置重名报错）
#[tauri::command]
pub fn add_custom_registry_preset(state: State<'_, AppState>, tool: String, name: String, url: String) -> Result<()> {
    state.manager.registry.writer(&tool)?;
    state.manager.add_custom_registry_preset(&tool, &name, &url)
}

/// 删除用户自定义镜像源
#[tauri::command]
pub fn remove_custom_registry_preset(state: State<'_, AppState>, tool: String, name: String) -> Result<()> {
    state.manager.remove_custom_registry_preset(&tool, &name)
}

/// 某工具当前镜像状态
#[tauri::command]
pub fn get_registry_state(state: State<'_, AppState>, tool: String) -> Result<RegistryState> {
    state.manager.registry_state(&tool)
}

/// 一键应用镜像（写入 + 自动备份）
#[tauri::command]
pub fn apply_registry(state: State<'_, AppState>, tool: String, preset: String) -> Result<()> {
    state.manager.apply_registry(&tool, &preset)
}

/// 下载加速镜像配置
#[tauri::command]
pub fn get_download_mirror(state: State<'_, AppState>) -> Result<crate::config::DownloadMirrorConfig> {
    Ok(state.manager.download_mirror_config())
}

/// 设置下载加速镜像（enable + 自定义前缀替换规则）
#[tauri::command]
pub fn set_download_mirror(
    state: State<'_, AppState>,
    enable: bool,
    rules: Option<std::collections::HashMap<String, String>>,
) -> Result<()> {
    state.manager.set_download_mirror(enable, rules)
}

/// 切换某 工具 的下载加速镜像（多地址切换）：`mirror` = 插件声明的镜像名；
/// None / "官方" → 该 工具 走官方源。返回更新后的完整镜像配置（前端直接覆盖）。
#[tauri::command]
pub fn set_tool_mirror(
    state: State<'_, AppState>,
    tool: String,
    mirror: Option<String>,
) -> Result<crate::config::DownloadMirrorConfig> {
    state.manager.set_tool_mirror(&tool, mirror.as_deref())
}
