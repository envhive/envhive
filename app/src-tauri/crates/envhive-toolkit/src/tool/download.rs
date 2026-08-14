//! 异步下载管道（roadmap D：reqwest + tokio，支持断点续传（HTTP Range）与进度回调）

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use envhive_core::events::{DownloadProgress, DownloadStage};

/// 下载进度回调：`Some(cb)` 时每个进度节流窗口调用一次（下载 / 校验阶段）。
/// 由调用方（manager 层）注入 Tauri 事件推送实现，保持本模块零 Tauri 依赖。
pub type ProgressCallback<'a> = Option<&'a (dyn Fn(&DownloadProgress) + Send + Sync)>;

/// 下载句柄：负责进度事件推送
pub struct Downloader<'a> {
    pub client: &'a reqwest::Client,
    /// 进度回调（`Some(cb)` 时推送进度；None = 静默下载，如 CLI / 后台同步）
    pub progress: ProgressCallback<'a>,
    pub tool: &'a str,
    pub version: &'a str,
    /// 取消标志（队列任务取消时置位；直接安装为 None）
    pub cancel: Option<Arc<AtomicBool>>,
}

/// 下载 URL 到目标文件；`dest.part` 记录已下载字节，支持断点续传。
/// 返回 (最终文件路径, 是否完整新下载)。
pub async fn download_to_file(
    dl: &Downloader<'_>,
    url: &str,
    dest: &Path,
) -> Result<PathBuf> {
    let part_path = part_path(dest);
    let partial_size = std::fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);

    // 断点续传：Range: bytes=N-
    let mut builder = dl.client.get(url);
    if partial_size > 0 {
        builder = builder.header(reqwest::header::RANGE, format!("bytes={partial_size}-"));
    }
    let resp = builder.send().await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::Network,
            format!("下载失败 HTTP {status}: {url}"),
        ));
    }

    // 服务端支持 Range 时返回 206；否则 200 需从头下载
    let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT;
    let total: Option<u64> = resp
        .content_length()
        .map(|l| l + if resumed { partial_size } else { 0 });

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(resumed)
        .write(true)
        .truncate(!resumed)
        .open(&part_path)?;

    let mut downloaded: u64 = if resumed { partial_size } else { 0 };
    let mut stream = resp.bytes_stream();
    let mut last_emit = Instant::now();
    let start = Instant::now();
    let mut last_bytes: u64 = downloaded;
    let mut last_time = start;

    tracing::info!("下载 {url}（续传={resumed}, 已有 {partial_size} 字节）");
    if let Some(cb) = dl.progress {
        cb(&DownloadProgress {
            tool: dl.tool.to_string(),
            version: dl.version.to_string(),
            percent: 0.0,
            speed_mbps: 0.0,
            stage: DownloadStage::Downloading,
            url: Some(url.to_string()),
            note: None,
            total_bytes: total,
            downloaded_bytes: downloaded,
        });
    }

    while let Some(chunk) = stream.next().await {
        // 用户取消：立即终止下载（保留 .part，断点续传可在下次继续）
        if dl.cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
            tracing::info!("下载已取消：{}（已下载 {} 字节，.part 保留供续传）", url, downloaded);
            return Err(EnvHiveError::new(
                EnvHiveErrorKind::Cancelled,
                format!("下载已取消（{downloaded} 字节）"),
            ));
        }
        let chunk = chunk?;
        downloaded += chunk.len() as u64;
        file.write_all(&chunk)?;

        // 进度节流：每 1s 或下载完成时推送（1s 窗口的瞬时速度更平滑，减少前端无效重绘）
        if last_emit.elapsed().as_millis() >= 1000 {
            let percent = total.map(|t| if t > 0 { (downloaded as f32 / t as f32) * 100.0 } else { 0.0 }).unwrap_or(0.0);
            let dt = last_time.elapsed().as_secs_f64();
            let speed_mbps = if dt > 0.0 {
                (downloaded - last_bytes) as f64 / dt / 1024.0 / 1024.0
            } else {
                0.0
            };
            if let Some(cb) = dl.progress {
                cb(&DownloadProgress {
                    tool: dl.tool.to_string(),
                    version: dl.version.to_string(),
                    percent,
                    speed_mbps,
                    stage: DownloadStage::Downloading,
                    url: Some(url.to_string()),
                    note: None,
                    total_bytes: total,
                    downloaded_bytes: downloaded,
                });
            }
            last_emit = Instant::now();
            last_bytes = downloaded;
            last_time = Instant::now();
        }
    }
    file.flush()?;
    drop(file);

    // 下载完成：.part → 正式文件
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    std::fs::rename(&part_path, dest)?;

    let percent = 100.0;
    let speed_mbps = {
        let dt = start.elapsed().as_secs_f64();
        if dt > 0.0 { downloaded as f64 / dt / 1024.0 / 1024.0 } else { 0.0 }
    };
    if let Some(cb) = dl.progress {
        cb(&DownloadProgress {
            tool: dl.tool.to_string(),
            version: dl.version.to_string(),
            percent,
            speed_mbps,
            // 下载完成最后一跳仍属「下载中」：Done 语义是「安装完成」，由
            // manager::install_tool 在解压/校验/验证全部结束后统一推送。
            // 此前这里提前发 Done，前端会误判安装完成（提前清进度条、刷新列表、
            // 弹"安装完成"通知），随后 Extracting 事件又把进度条钉在「解压安装」。
            stage: DownloadStage::Downloading,
            url: Some(url.to_string()),
            note: None,
            total_bytes: total,
            downloaded_bytes: downloaded,
        });
    }
    tracing::info!("下载完成：{}（{} 字节）", dest.display(), downloaded);
    Ok(dest.to_path_buf())
}

/// `.part` 临时文件路径
pub fn part_path(dest: &Path) -> PathBuf {
    let mut os = dest.as_os_str().to_os_string();
    os.push(".part");
    PathBuf::from(os)
}

/// 清理 `.part` 残file（失败时调用）
pub fn cleanup_part(dest: &Path) {
    let _ = std::fs::remove_file(part_path(dest));
}

/// 下载并校验：Download → Checksum.Verify（roadmap D 管道第一步）
pub async fn download_and_verify(
    dl: &Downloader<'_>,
    url: &str,
    dest: &Path,
    checksum_spec: Option<&str>,
) -> Result<PathBuf> {
    let file = download_to_file(dl, url, dest).await?;

    // 完整性校验（None / "none" 跳过）
    if let Some(spec) = checksum_spec {
        if let Some(cb) = dl.progress {
            cb(&DownloadProgress {
                tool: dl.tool.to_string(),
                version: dl.version.to_string(),
                percent: 100.0,
                speed_mbps: 0.0,
                stage: DownloadStage::Verifying,
                url: Some(url.to_string()),
                note: None,
                total_bytes: None,
                downloaded_bytes: 0,
            });
        }
        crate::tool::checksum::verify_file(&file, spec)?;
    }
    Ok(file)
}
