//! 日志系统（roadmap J）：tracing 分级落盘到 `~/.envhive/logs/`，按天滚动。
//!
//! 保留策略：启动时清理超过 [`DEFAULT_LOG_RETENTION_DAYS`] 天的历史日志（按文件 mtime 判断），
//! 仅删除按天滚动产物（`envhive.log.YYYY-MM-DD`），失败只告警不影响启动。

use std::path::Path;

/// 日志默认保留天数：超出即清理
pub const DEFAULT_LOG_RETENTION_DAYS: u64 = 14;

pub fn init(logs_dir: &Path) {
    if let Err(e) = std::fs::create_dir_all(logs_dir) {
        eprintln!("[envhive] 创建日志目录失败: {e}");
        return;
    }
    prune_old_logs(logs_dir, DEFAULT_LOG_RETENTION_DAYS);
    let file = tracing_appender::rolling::daily(logs_dir, "envhive.log");
    // 默认仅 info 及以上级别落盘，debug 不写入日志文件。
    // 排障时可临时通过环境变量 RUST_LOG=debug 打开，无需改代码。
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(file)
        .with_ansi(false)
        .with_target(false)
        .init();
    tracing::info!("envhive 日志系统初始化完成，目录: {}", logs_dir.display());
}

/// 清理超过保留期的历史日志（按 mtime 判断，保留最近 `retention_days` 天）。
/// 尽力而为：删除失败仅 warn（如文件被其他进程占用），不阻塞启动。
pub fn prune_old_logs(logs_dir: &Path, retention_days: u64) {
    const SECS_PER_DAY: u64 = 86400;
    if retention_days == 0 {
        return;
    }
    let now = std::time::SystemTime::now();
    let Ok(entries) = std::fs::read_dir(logs_dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // 仅清理按天滚动产物（如 envhive.log.2026-09-09），保留无日期后缀的当前文件 / 非日志文件
        if !name.starts_with("envhive.log") || name.len() <= "envhive.log".len() {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else { continue };
        let Ok(age) = now.duration_since(modified) else { continue };
        if age.as_secs() > retention_days * SECS_PER_DAY {
            match std::fs::remove_file(entry.path()) {
                Ok(()) => tracing::info!("已清理超过 {retention_days} 天的日志: {}", entry.path().display()),
                Err(e) => tracing::warn!("清理过期日志失败（可能被占用）: {}: {e}", entry.path().display()),
            }
        }
    }
}
