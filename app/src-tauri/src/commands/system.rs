//! 系统域命令：自启动 / 托盘 / 使用统计 / 冲突检测 / 导入导出 / 系统环境变量

use tauri::State;

use crate::error::Result;

use super::AppState;

/// 开机自启动实际状态
#[tauri::command]
pub fn get_autostart(state: State<'_, AppState>) -> Result<bool> {
    Ok(state.manager.autostart_actual())
}

/// 设置开机自启动（写系统 Run 键 / autostart 文件 + 同步 config.yaml）
#[tauri::command]
pub fn set_autostart(state: State<'_, AppState>, enable: bool) -> Result<()> {
    state.manager.set_autostart(enable)
}

/// 托盘常驻状态（关闭窗口 → 隐藏到托盘，后台继续运行）
#[tauri::command]
pub fn get_tray_resident(state: State<'_, AppState>) -> Result<bool> {
    Ok(state.manager.tray_resident())
}

/// 设置托盘常驻（写 config.yaml，运行期即时生效）
#[tauri::command]
pub fn set_tray_resident(state: State<'_, AppState>, enable: bool) -> Result<()> {
    state.manager.set_tray_resident(enable)
}

/// 使用统计（各 工具 / 版本使用频率 + 磁盘占用，辅助释放磁盘）
#[tauri::command]
pub fn get_usage_stats(state: State<'_, AppState>) -> Result<crate::usage::UsageStats> {
    crate::usage::stats(&state.manager)
}

/// 全部冲突（registry 配置文件外部修改 + 工具 声明与链接不一致）
#[tauri::command]
pub fn check_conflicts(state: State<'_, AppState>) -> Result<Vec<crate::extras::ConflictInfo>> {
    Ok(crate::extras::check_all_conflicts(&state.manager.paths, &state.manager))
}

/// 重新记录某工具的配置文件指纹（用户确认覆盖后调用，解除冲突提示）
#[tauri::command]
pub fn ack_registry_conflict(state: State<'_, AppState>, tool: String) -> Result<()> {
    let w = state.manager.registry.writer(&tool)?;
    let path = w.config_path()?;
    crate::extras::record_registry_fingerprint(&state.manager.paths, &tool, &path);
    Ok(())
}

/// 导出当前环境为 YAML / JSON
#[tauri::command]
pub fn export_env(state: State<'_, AppState>, format: Option<String>) -> Result<String> {
    crate::extras::export_env(&state.manager, format.as_deref().unwrap_or("yaml"))
}

/// 导入环境快照（YAML/JSON 自动识别）并应用
#[tauri::command]
pub fn import_env(state: State<'_, AppState>, content: String) -> Result<()> {
    crate::extras::import_env(&state.manager, &content)
}

/// 应用为系统环境变量（写系统变量：Windows 注册表 PATH + JAVA_HOME 等 + 广播）
#[tauri::command]
pub fn apply_global_env(state: State<'_, AppState>) -> Result<crate::manager::ApplyGlobalResult> {
    state.manager.apply_global_env()
}
