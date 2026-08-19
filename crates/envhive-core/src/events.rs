//! 事件数据类型（envhive-core）
//!
//! 纯数据类型（零 Tauri 依赖），供 toolkit（下载进度回调）与 app 层（events::emit_* 推送）共用。
//! 事件名约定：`download-progress` / `install-status` / `app-error` / `version-changed`

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadStage {
    /// 解析版本 / 包地址
    Resolving,
    /// 下载中
    Downloading,
    /// 完整性校验
    Verifying,
    /// 解压安装
    Extracting,
    /// 完成
    Done,
    /// 失败
    Failed,
}

/// 下载进度事件负载（roadmap I 定义）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub tool: String,
    pub version: String,
    pub percent: f32,
    pub speed_mbps: f64,
    pub stage: DownloadStage,
    /// 实际下载地址（镜像替换后的最终 URL；下载阶段有效，其余阶段为 None）
    pub url: Option<String>,
    /// 附加提示（如「镜像下载失败，已回退官方源」；无则为 None）
    pub note: Option<String>,
    /// 下载文件总大小（字节；服务器未提供 Content-Length 时为 None）
    pub total_bytes: Option<u64>,
    /// 已下载字节数（断点续传时含此前已下载部分）
    pub downloaded_bytes: u64,
}

/// 安装状态事件（阶段文本 + 详情）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallStatus {
    pub tool: String,
    pub version: String,
    pub stage: String,
    pub detail: Option<String>,
}

/// 版本切换成功事件
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionChanged {
    pub tool: String,
    pub version: String,
}
