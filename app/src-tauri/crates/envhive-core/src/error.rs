//! 统一错误模型 —— 所有 `#[tauri::command]` 返回 `Result<T, EnvHiveError>`。
//! 序列化为 `{ code, message }`，前端可据 `code` 做可操作提示（对应 roadmap 模块 J）。

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use thiserror::Error;

/// 结构化错误码（roadmap J：vfox NotFoundError 模式，前端据 code 展示可操作提示）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EnvHiveErrorKind {
    /// 全局配置 / 配置文件错误
    Config,
    /// 文件系统错误
    Io,
    /// 网络 / 下载错误
    Network,
    /// 完整性校验失败
    Checksum,
    /// 解压失败
    Decompress,
    /// 工具 未注册 / 未找到（可恢复错误，上层可触发自动安装）
    ToolNotFound,
    /// 版本不存在于可用列表
    VersionNotFound,
    /// 模糊版本解析失败（精确/前缀/标签均未命中）
    VersionResolve,
    /// 版本尚未安装
    NotInstalled,
    /// 安装流程失败
    Install,
    /// 平台 / 压缩格式暂不支持
    Unsupported,
    /// 系统链接 / 注册表操作失败
    Link,
    /// 镜像源 / 仓库配置写入或验证失败（P1）
    Registry,
    /// 任务被用户取消（下载队列 / 下载中取消）
    Cancelled,
    /// 其他内部错误
    Internal,
}

impl EnvHiveErrorKind {
    pub fn code(&self) -> &'static str {
        match self {
            EnvHiveErrorKind::Config => "CONFIG_ERROR",
            EnvHiveErrorKind::Io => "IO_ERROR",
            EnvHiveErrorKind::Network => "NETWORK_ERROR",
            EnvHiveErrorKind::Checksum => "CHECKSUM_MISMATCH",
            EnvHiveErrorKind::Decompress => "DECOMPRESS_ERROR",
            EnvHiveErrorKind::ToolNotFound => "TOOL_NOT_FOUND",
            EnvHiveErrorKind::VersionNotFound => "VERSION_NOT_FOUND",
            EnvHiveErrorKind::VersionResolve => "VERSION_RESOLVE_ERROR",
            EnvHiveErrorKind::NotInstalled => "NOT_INSTALLED",
            EnvHiveErrorKind::Install => "INSTALL_ERROR",
            EnvHiveErrorKind::Unsupported => "UNSUPPORTED",
            EnvHiveErrorKind::Link => "LINK_ERROR",
            EnvHiveErrorKind::Registry => "REGISTRY_ERROR",
            EnvHiveErrorKind::Cancelled => "CANCELLED",
            EnvHiveErrorKind::Internal => "INTERNAL_ERROR",
        }
    }
}

#[derive(Debug, Error)]
#[error("[{kind:?}] {message}")]
pub struct EnvHiveError {
    pub kind: EnvHiveErrorKind,
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl Serialize for EnvHiveError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::Error as _;
        let mut st = serializer.serialize_struct("EnvHiveError", 2)?;
        st.serialize_field("code", &self.kind.code()).map_err(S::Error::custom)?;
        st.serialize_field("message", &self.message).map_err(S::Error::custom)?;
        st.end().map_err(S::Error::custom)
    }
}

impl EnvHiveError {
    pub fn new(kind: EnvHiveErrorKind, message: impl Into<String>) -> Self {
        EnvHiveError { kind, message: message.into(), source: None }
    }

    pub fn with_source(
        kind: EnvHiveErrorKind,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        EnvHiveError {
            kind,
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    #[allow(dead_code)]
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }
}

impl From<std::io::Error> for EnvHiveError {
    fn from(e: std::io::Error) -> Self {
        EnvHiveError::with_source(EnvHiveErrorKind::Io, e.to_string(), e)
    }
}

impl From<reqwest::Error> for EnvHiveError {
    fn from(e: reqwest::Error) -> Self {
        EnvHiveError::with_source(EnvHiveErrorKind::Network, format!("网络请求失败: {e}"), e)
    }
}

impl From<serde_yaml::Error> for EnvHiveError {
    fn from(e: serde_yaml::Error) -> Self {
        EnvHiveError::with_source(EnvHiveErrorKind::Config, format!("config.yaml 解析失败: {e}"), e)
    }
}

impl From<toml::de::Error> for EnvHiveError {
    fn from(e: toml::de::Error) -> Self {
        EnvHiveError::with_source(EnvHiveErrorKind::Config, format!("TOML 解析失败: {e}"), e)
    }
}

impl From<toml::ser::Error> for EnvHiveError {
    fn from(e: toml::ser::Error) -> Self {
        EnvHiveError::with_source(EnvHiveErrorKind::Config, format!("TOML 序列化失败: {e}"), e)
    }
}

impl From<serde_json::Error> for EnvHiveError {
    fn from(e: serde_json::Error) -> Self {
        EnvHiveError::with_source(EnvHiveErrorKind::Internal, format!("JSON 处理失败: {e}"), e)
    }
}

pub type Result<T> = std::result::Result<T, EnvHiveError>;

/// 便捷构造宏：`bail!(Config, "消息 {}", x)` 返回 `Err(EnvHiveError)`
#[macro_export]
macro_rules! bail {
    ($kind:expr, $($arg:tt)*) => {
        return Err($crate::error::EnvHiveError::new($kind, format!($($arg)*)))
    };
}
