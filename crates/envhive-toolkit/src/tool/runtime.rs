//! 运行时类型（roadmap A：Runtime / RuntimePackage / 可用版本）

use serde::{Deserialize, Serialize};

/// 单文件二进制 工具 的附加文件（如 Windows 上 exe 同目录所需的 dll）：
/// 随主文件一并由宿主下载并放入安装目录，仅 Binary 类型 工具 使用。
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExtraFile {
    pub url: String,
    pub file_name: String,
    /// 校验信息：`sha256:<hex>` 等；`None` = 跳过校验
    pub checksum: Option<String>,
}

/// 一个可下载的运行时包
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimePackage {
    pub tool: String,
    pub version: String,
    pub url: String,
    /// 校验信息：`sha256:<hex>` / `sha512:<hex>` / 等；`None` = 跳过校验
    pub checksum: Option<String>,
    /// 归档文件下载后保存的文件名
    pub file_name: String,
    /// 附加文件（可选；单文件 工具 的伴随资源，如 Windows dll）
    #[serde(default)]
    pub extra_files: Vec<ExtraFile>,
}

/// 可用版本（版本列表接口返回）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableVersion {
    pub version: String,
    #[serde(default)]
    pub lts: bool,
    /// 附加标签：latest / lts / stable
    #[serde(default)]
    pub labels: Vec<String>,
}

/// 版本列表缓存内容（`cache/versions/<tool>.json`）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionCache {
    /// 抓取时间（unix 秒）
    pub fetched_at: i64,
    pub versions: Vec<AvailableVersion>,
}

impl VersionCache {
    pub fn version_strings(&self) -> Vec<String> {
        self.versions.iter().map(|v| v.version.clone()).collect()
    }
}
