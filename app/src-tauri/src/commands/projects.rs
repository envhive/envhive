//! 项目域命令：项目预设（列表 / 保存 / 删除 / 会话启动）

use tauri::State;

use crate::error::Result;

use super::AppState;

/// 项目预设列表（首页「项目环境」区）
#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> Result<Vec<crate::projects::ProjectPreset>> {
    state.manager.list_projects()
}

/// 保存 / 新建项目预设（同名覆盖）
#[tauri::command]
pub fn save_project(
    state: State<'_, AppState>,
    preset: crate::projects::ProjectPreset,
) -> Result<()> {
    state.manager.save_project(preset)
}

/// 删除项目预设
#[tauri::command]
pub fn delete_project(state: State<'_, AppState>, name: String) -> Result<()> {
    state.manager.delete_project(&name)
}

/// 按项目预设启动会话（注入该组合的环境变量：终端 / IDE / 开发服务器）
#[tauri::command]
pub fn launch_session(
    state: State<'_, AppState>,
    project: String,
    command: String,
    args: Vec<String>,
) -> Result<()> {
    state.manager.launch_session(&project, &command, &args)
}
