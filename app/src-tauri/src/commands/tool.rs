//! 工具 域命令：列表 / 详情 / 版本 / 安装 / 卸载 / 全局切换 / 下载队列

use tauri::{AppHandle, State};

use crate::error::{EnvHiveError, Result};
use crate::tool::{InstalledInfo, InstallResult, ToolInfo, SwitchResult};

use super::AppState;

/// 工具 列表（首页卡片）
#[tauri::command]
pub fn list_tools(state: State<'_, AppState>) -> Result<Vec<ToolInfo>> {
    state.manager.list_tools(None)
}

/// 单 工具 详情
#[tauri::command]
pub fn get_tool(state: State<'_, AppState>, name: String) -> Result<ToolInfo> {
    state.manager.lookup_tool(&name)?;
    let info = state
        .manager
        .list_tools(None)?
        .into_iter()
        .find(|s| s.name == name.to_ascii_lowercase())
        .ok_or_else(|| EnvHiveError::new(crate::error::EnvHiveErrorKind::ToolNotFound, format!("工具 {name} 未注册")))?;
    Ok(info)
}

/// 可用版本列表（带 TTL 缓存；refresh=true 强制刷新）
/// v2：`distribution` = 发行商 key（Lua 插件发行商维度；无发行商 工具 传 null）
#[tauri::command]
pub async fn get_versions(
    state: State<'_, AppState>,
    name: String,
    distribution: Option<String>,
    refresh: Option<bool>,
) -> Result<Vec<String>> {
    let cache = state
        .manager
        .fetch_versions(&name, distribution.as_deref(), refresh.unwrap_or(false))
        .await?;
    Ok(cache.version_strings())
}

/// 搜索 / 过滤（roadmap E：search_sdk —— 在缓存版本中按版本号与标签过滤）
#[tauri::command]
pub fn search_versions(
    state: State<'_, AppState>,
    name: String,
    distribution: Option<String>,
    filter: Option<String>,
) -> Result<Vec<String>> {
    let tool = state.manager.lookup_tool(&name)?;
    let filter = filter.unwrap_or_default().trim().to_ascii_lowercase();
    // 从内存缓存读取（未拉取则返回空，前端先调 get_versions）
    let cached = state.manager.version_cache_snapshot(tool.name(), distribution.as_deref());
    let mut out: Vec<String> = cached
        .versions
        .iter()
        .filter(|v| {
            filter.is_empty()
                || v.version.to_ascii_lowercase().contains(&filter)
                || v.labels.iter().any(|l| l.to_ascii_lowercase().contains(&filter))
        })
        .map(|v| v.version.clone())
        .collect();
    crate::tool::version::sort_versions(&mut out);
    Ok(out)
}

/// 安装 工具（异步，进度经事件总线推送）
/// v2：`distribution` = 发行商 key（Lua 插件发行商维度）
#[tauri::command]
pub async fn install_tool(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    version: String,
    distribution: Option<String>,
) -> Result<InstallResult> {
    state.manager.install_tool(&app, &name, &version, distribution.as_deref(), None).await
}

/// 卸载 工具 版本
#[tauri::command]
pub fn uninstall_tool(state: State<'_, AppState>, name: String, version: String) -> Result<()> {
    state.manager.uninstall_tool(&name, &version)
}

/// 全局切换版本（写 Global TOML + 重建链接 + 注册表 PATH）
#[tauri::command]
pub async fn switch_version(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    version: String,
) -> Result<SwitchResult> {
    state.manager.switch_global(&app, &name, &version).await
}

/// 解除全局使用
#[tauri::command]
pub fn unuse_global(state: State<'_, AppState>, name: String) -> Result<()> {
    state.manager.unuse_global(&name)
}

/// 当前版本查询
#[tauri::command]
pub fn current_tool(state: State<'_, AppState>, name: String) -> Result<Option<String>> {
    let chain = state.manager.chain(None)?;
    let tool = state.manager.lookup_tool(&name)?;
    Ok(chain.tool(tool.name()).map(|t| t.version().to_string()))
}

/// 已安装列表
#[tauri::command]
pub fn list_installed(state: State<'_, AppState>) -> Result<Vec<InstalledInfo>> {
    Ok(state.manager.list_installed())
}

/// 入队安装（下载队列；批量安装）
/// v2：`distribution` = 发行商 key（Lua 插件发行商维度）
/// 去重：队列中已有同 tool+version+distribution 的排队/执行中任务时直接复用（reused=true），不重复入队。
#[tauri::command]
pub fn enqueue_install(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    version: String,
    distribution: Option<String>,
) -> Result<crate::queue::EnqueueOutcome> {
    let outcome = state.queue.enqueue(name, version, distribution);
    // 入队后立即推送队列快照：让前端实时看到新任务（含「排队中」状态）。
    // 否则新任务要等 worker 状态切换（running/done）才有事件，多个任务连续添加时
    // 队列页始终只显示当前正在执行的那一个。
    state.queue.emit_snapshot(&app);
    // 复用已有任务时无需再 spawn worker（原任务已在队列中，worker 会消费到它）
    if !outcome.reused {
        let queue = state.queue.clone();
        let manager = state.manager.clone();
        tauri::async_runtime::spawn(async move {
            queue.run_worker(app, manager).await;
        });
    }
    Ok(outcome)
}

/// 队列状态
#[tauri::command]
pub fn queue_status(state: State<'_, AppState>) -> Result<Vec<crate::queue::QueueTask>> {
    Ok(state.queue.snapshot())
}

/// 取消排队中的任务
#[tauri::command]
pub fn cancel_task(state: State<'_, AppState>, id: u64) -> Result<()> {
    state.queue.cancel(id)
}

/// 全部取消：排队中直接取消；执行中请求取消（worker 收尾定终态）。返回受影响任务数。
#[tauri::command]
pub fn queue_cancel_all(app: AppHandle, state: State<'_, AppState>) -> Result<usize> {
    let n = state.queue.cancel_all();
    state.queue.emit_snapshot(&app);
    Ok(n)
}

/// 清空终态任务（Done/Failed/Cancelled），保留排队中/执行中。返回移除数量。
#[tauri::command]
pub fn queue_clear_finished(app: AppHandle, state: State<'_, AppState>) -> Result<usize> {
    let n = state.queue.clear_finished();
    state.queue.emit_snapshot(&app);
    Ok(n)
}
