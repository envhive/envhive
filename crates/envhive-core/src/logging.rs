//! 日志系统（roadmap J）：tracing 分级落盘到 `~/.envhive/logs/`，按天滚动。

use std::path::Path;

pub fn init(logs_dir: &Path) {
    if let Err(e) = std::fs::create_dir_all(logs_dir) {
        eprintln!("[envhive] 创建日志目录失败: {e}");
        return;
    }
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
