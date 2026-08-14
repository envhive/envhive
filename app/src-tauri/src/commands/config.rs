//! 配置域命令：全局配置 / 日志 / 目录 / 代理

use serde::Serialize;
use tauri::State;

use crate::error::{EnvHiveError, EnvHiveErrorKind, Result};

use super::AppState;

/// 全局配置（供设置页）
#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Result<serde_json::Value> {
    let cfg = state.manager.config.lock().unwrap();
    serde_json::to_value(&*cfg)
        .map_err(|e| EnvHiveError::new(crate::error::EnvHiveErrorKind::Internal, format!("序列化配置失败: {e}")))
}

/// 更新全局配置（设置页）：仅更新传入的非空字段，其余保持不变。
/// - `cache_ttl`：版本缓存 TTL（如 "12h" / "0" / "-1"）
/// - `registry_entries`：远程插件仓库列表（name = 仓库名 + url = manifest.json 完整地址；至少一个）
/// - `storage_path`：工具 安装根目录（⚠️ 修改后需重启应用生效）
#[tauri::command]
pub fn update_config(
    state: State<'_, AppState>,
    cache_ttl: Option<String>,
    registry_entries: Option<Vec<envhive_core::config::RegistryEntry>>,
    storage_path: Option<String>,
) -> Result<()> {
    {
        let mut cfg = state.manager.config.lock().unwrap();
        let mut changed = false;
        if let Some(ttl) = cache_ttl {
            let ttl = ttl.trim().to_string();
            // 简单校验格式（数字 / 数字+单位 / -1 / 0）
            if ttl != "-1" && ttl != "0" {
                let (num, unit) = ttl.split_at(ttl.len().saturating_sub(1));
                let unit_ok = matches!(unit, "h" | "m" | "s" | "d") || num.parse::<i64>().is_ok();
                if num.parse::<i64>().is_err() || !unit_ok {
                    return Err(EnvHiveError::new(
                        EnvHiveErrorKind::Config,
                        format!("非法缓存 TTL: {ttl:?}（示例: 12h / 3600 / -1 / 0）"),
                    ));
                }
            }
            if cfg.cache.available_hook_duration != ttl {
                cfg.cache.available_hook_duration = ttl;
                changed = true;
            }
        }
        if let Some(entries) = registry_entries {
            // 校验 name 非空 + url 协议合法，过滤空白 / 末尾斜杠，按 url 去重；至少保留一个
            let mut seen = std::collections::HashSet::new();
            let mut cleaned: Vec<envhive_core::config::RegistryAddress> = Vec::new();
            for raw in entries {
                let url = raw.url.trim().trim_end_matches('/').to_string();
                if url.is_empty() {
                    continue;
                }
                if !url.starts_with("https://") && !url.starts_with("http://") {
                    return Err(EnvHiveError::new(
                        EnvHiveErrorKind::Config,
                        "插件仓库地址需以 http:// 或 https:// 开头",
                    ));
                }
                let name = raw.name.trim().to_string();
                if name.is_empty() {
                    return Err(EnvHiveError::new(
                        EnvHiveErrorKind::Config,
                        "插件仓库名不能为空（如「官方gitee」「官方github」）",
                    ));
                }
                if seen.insert(url.clone()) {
                    cleaned.push(envhive_core::config::RegistryAddress::Named(
                        envhive_core::config::RegistryEntry { name, url },
                    ));
                }
            }
            if cleaned.is_empty() {
                return Err(EnvHiveError::new(
                    EnvHiveErrorKind::Config,
                    "至少需要配置一个插件仓库地址",
                ));
            }
            if cfg.registry.addresses != cleaned {
                cfg.registry.addresses = cleaned;
                cfg.registry.address = None; // 旧单字段迁移完成，不再写回
                // 记忆的选中仓库被删除时清除，前端回退到列表第一个
                if let Some(sel) = &cfg.registry.selected {
                    let still_exists = cfg.registry.addresses.iter().any(|a| match a {
                        envhive_core::config::RegistryAddress::Named(e) => e.url == *sel,
                        envhive_core::config::RegistryAddress::Plain(u) => u == sel,
                    });
                    if !still_exists {
                        cfg.registry.selected = None;
                    }
                }
                changed = true;
            }
        }
        if let Some(sp) = storage_path {
            let sp = sp.trim().to_string();
            if sp.is_empty() {
                return Err(EnvHiveError::new(EnvHiveErrorKind::Config, "存储路径不能为空"));
            }
            if cfg.storage.tool_path != sp {
                cfg.storage.tool_path = sp;
                changed = true;
            }
        }
        if changed {
            cfg.save(&state.manager.paths.config_file)?;
            tracing::info!("update_config: 配置已更新并保存");
        }
    }
    Ok(())
}

/// 记住插件市场上次切换选中的仓库 URL（写入 config.yaml registry.selected；
/// 启动时前端据此默认展示该仓库）。url 为空则清除记忆（回退列表第一个）。
#[tauri::command]
pub fn set_registry_selected(state: State<'_, AppState>, url: Option<String>) -> Result<()> {
    let mut cfg = state.manager.config.lock().unwrap();
    let url = url
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .filter(|u| !u.is_empty());
    if cfg.registry.selected != url {
        cfg.registry.selected = url;
        cfg.save(&state.manager.paths.config_file)?;
    }
    Ok(())
}

/// 日志读取（设置页「导出日志」）：列出日志目录文件（mtime 倒序）+ 最新文件尾部内容
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsInfo {
    /// 日志文件列表（文件名，最新在前）
    pub files: Vec<String>,
    /// 最新日志文件的尾部内容（最多 200KB）
    pub content: String,
    /// 日志目录
    pub logs_dir: String,
}

#[tauri::command]
pub fn get_logs(state: State<'_, AppState>) -> Result<LogsInfo> {
    let dir = &state.manager.paths.logs;
    let mut files: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("envhive.log") {
                continue;
            }
            let mtime = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            files.push((mtime, e.path()));
        }
    }
    files.sort_by(|a, b| b.0.cmp(&a.0)); // 最新在前
    let names: Vec<String> = files.iter().map(|(_, p)| p.file_name().unwrap_or_default().to_string_lossy().to_string()).collect();

    // 读取最新文件尾部（截断 200KB）
    let content = match files.first() {
        Some((_, path)) => {
            let meta = std::fs::metadata(path).ok();
            let len = meta.map(|m| m.len()).unwrap_or(0);
            let mut s = String::new();
            if len > 200 * 1024 {
                // 从尾部 200KB 读取
                if let Ok(f) = std::fs::File::open(path) {
                    use std::io::{Read, Seek, SeekFrom};
                    let mut f = f;
                    let _ = f.seek(SeekFrom::Start(len - 200 * 1024));
                    let _ = f.read_to_string(&mut s);
                    s = format!("…（截断，共 {}KB）\n{}", len / 1024, s);
                }
            } else if let Ok(c) = std::fs::read_to_string(path) {
                s = c;
            }
            s
        }
        None => String::new(),
    };

    Ok(LogsInfo {
        files: names,
        content,
        logs_dir: dir.to_string_lossy().to_string(),
    })
}

/// 关键目录（日志导出 / 配置展示）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub root: String,
    pub config_file: String,
    pub logs_dir: String,
    pub install_dir: String,
    pub tools_dir: String,
}

#[tauri::command]
pub fn get_app_paths(state: State<'_, AppState>) -> Result<AppPaths> {
    Ok(AppPaths {
        root: state.manager.paths.root.to_string_lossy().to_string(),
        config_file: state.manager.paths.config_file.to_string_lossy().to_string(),
        logs_dir: state.manager.paths.logs.to_string_lossy().to_string(),
        install_dir: state.manager.paths.install_root.to_string_lossy().to_string(),
        tools_dir: state.manager.paths.tools.to_string_lossy().to_string(),
    })
}

/// 启动自检：返回环境信息（前端可展示运行状态）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapInfo {
    pub version: String,
    pub platform: String,
    pub arch: String,
    pub install_dir: String,
    pub tools_dir: String,
    pub logs_dir: String,
    pub config_writable: bool,
}

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> Result<BootstrapInfo> {
    let writable = state.manager.config.lock().unwrap().verify_writable().is_ok();
    Ok(BootstrapInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        install_dir: state.manager.paths.install_root.to_string_lossy().to_string(),
        tools_dir: state.manager.paths.tools.to_string_lossy().to_string(),
        logs_dir: state.manager.paths.logs.to_string_lossy().to_string(),
        config_writable: writable,
    })
}

/// 当前代理配置
#[tauri::command]
pub fn get_proxy(state: State<'_, AppState>) -> Result<crate::config::ProxyConfig> {
    Ok(state.manager.config.lock().unwrap().proxy.clone())
}

/// 设置代理（写 config + 重建下载 client + 工具代理；仅应用内生效）
#[tauri::command]
pub fn set_proxy(
    state: State<'_, AppState>,
    url: Option<String>,
    enable: bool,
) -> Result<()> {
    state.manager.set_proxy(url, enable)
}

/// 日志文件列表（名称 + 大小 + 修改时间，最新在前）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFileInfo {
    pub name: String,
    pub size: u64,
    pub modified_at: i64,
}

#[tauri::command]
pub fn list_logs(state: State<'_, AppState>) -> Result<Vec<LogFileInfo>> {
    let dir = &state.manager.paths.logs;
    let mut files: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("envhive.log") {
                continue;
            }
            let mtime = e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            files.push((mtime, e.path()));
        }
    }
    files.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(files
        .into_iter()
        .map(|(mt, p)| {
            let meta = std::fs::metadata(&p).ok();
            LogFileInfo {
                name: p.file_name().unwrap_or_default().to_string_lossy().to_string(),
                size: meta.map(|m| m.len()).unwrap_or(0),
                modified_at: mt
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
            }
        })
        .collect())
}

/// 读取日志内容（指定文件 + 级别过滤 + 尾部截断 300KB；file 为空取最新）
#[tauri::command]
pub fn read_logs(
    state: State<'_, AppState>,
    file: Option<String>,
    level: Option<String>,
) -> Result<String> {
    let dir = &state.manager.paths.logs;
    let mut files: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("envhive.log") {
                continue;
            }
            let mtime = e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            files.push((mtime, e.path()));
        }
    }
    files.sort_by(|a, b| b.0.cmp(&a.0)); // 最新在前

    let path = match file.as_deref().filter(|f| !f.is_empty()) {
        Some(name) => files.iter().find(|(_, p)| p.file_name().unwrap_or_default() == name).map(|(_, p)| p.clone()),
        None => files.first().map(|(_, p)| p.clone()),
    };
    let Some(path) = path else { return Ok(String::new()) };

    // 读取尾部 300KB
    let meta = std::fs::metadata(&path).ok();
    let len = meta.map(|m| m.len()).unwrap_or(0);
    let mut raw = String::new();
    if len > 300 * 1024 {
        if let Ok(f) = std::fs::File::open(&path) {
            use std::io::{Read, Seek, SeekFrom};
            let mut f = f;
            let _ = f.seek(SeekFrom::Start(len - 300 * 1024));
            let _ = f.read_to_string(&mut raw);
            raw = format!("…（截断，共 {}KB）\n{}", len / 1024, raw);
        }
    } else if let Ok(c) = std::fs::read_to_string(&path) {
        raw = c;
    }

    // 级别过滤：WARN 包含 WARN+ERROR；ERROR 仅 ERROR；其余不滤
    let level = level.as_deref().unwrap_or("ALL").trim().to_ascii_uppercase();
    if level == "ALL" || level.is_empty() {
        return Ok(raw);
    }
    let min_rank = match level.as_str() {
        "ERROR" => 3,
        "WARN" => 2,
        "INFO" => 1,
        _ => 0,
    };
    let rank = |line: &str| -> u8 {
        if line.contains(" ERROR ") || line.contains(" ERROR]") || line.starts_with("ERROR") {
            3
        } else if line.contains(" WARN ") || line.contains(" WARN]") {
            2
        } else if line.contains(" INFO ") || line.contains(" INFO]") {
            1
        } else {
            0
        }
    };
    Ok(raw
        .lines()
        .filter(|l| rank(l) >= min_rank)
        .collect::<Vec<_>>()
        .join("\n"))
}
