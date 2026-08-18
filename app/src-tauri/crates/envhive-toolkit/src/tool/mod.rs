//! 工具 层（roadmap A/G：Tool trait 与 工具 描述抽象）
//! 工具 来源统一为插件：Lua 插件（plugin.lua，完整脚本能力）。
//! 内置 8 个 工具（nodejs/java/go/rust/python/maven/tomcat/lua）以 Lua 插件
//! 形式从插件仓库获取，用户可自由修改/移除。

pub mod checksum;
pub mod download;
pub mod icon;
pub mod install;
pub mod provider;
pub mod resolver;
pub mod runtime;
pub mod version;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use envhive_core::env::symlink;
use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use envhive_core::pathmeta::PathMeta;
use crate::tool::install as install_mod;
use crate::tool::provider::ProviderKind;
use crate::tool::version::Version;

/// 环境变量注入定义：JAVA_HOME 等
#[derive(Debug, Clone, Copy)]
pub enum EnvVarKind {
    /// 指向版本根目录（如 JAVA_HOME=current）
    RootDir,
}

/// 平台映射：os 别名 + arch 别名（插件定义 platform 声明）
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct PlatformMap {
    pub os: String,
    pub arch: HashMap<String, String>,
}

/// 发行商信息（TOOL.distributions 声明；前端据此渲染「发行商」下拉）
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DistributionInfo {
    pub key: String,
    pub display: String,
}

/// 下载加速镜像候选（TOOL.mirrors 声明；前端据此渲染「加速镜像」下拉，
/// 多个候选可切换）。`from` = 插件生成 URL 的官方前缀，`to` = 镜像基址，前缀命中即替换。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MirrorCandidate {
    pub name: String,
    pub from: String,
    pub to: String,
}

/// 工具 描述抽象：Lua 插件（LuaPluginDef）实现
pub trait ToolDescriptor: Send + Sync {
    fn name(&self) -> &str;
    fn display(&self) -> &str;
    fn category(&self) -> &str;
    fn homepage(&self) -> &str;
    fn provider(&self) -> ProviderKind;
    fn gh_repo(&self) -> Option<(&str, &str)>;
    fn static_index_url(&self) -> Option<&str>;
    /// 插件专用：下载 URL 模板（{version} {os} {arch} {ext} 占位符）
    fn url_template(&self) -> Option<&str>;
    /// 插件专用：平台映射
    fn platform(&self) -> Option<&crate::tool::PlatformMap>;
    fn verify_bin(&self) -> &str;
    fn verify_arg(&self) -> &str;
    fn bin_suffix(&self) -> &str;
    fn env_vars(&self) -> &[(&str, EnvVarKind)];
    fn root_hint(&self) -> Option<&str>;
    /// Lua 插件：发行商维度（声明则前端渲染「发行商」下拉；无则无此维度）
    fn distributions(&self) -> &[DistributionInfo] {
        &[]
    }
    /// Lua 插件：缺省发行商 key（缺省取 distributions[0]）
    fn default_distribution(&self) -> Option<&str> {
        None
    }
    /// Lua 插件：下载加速镜像候选（TOOL.mirrors 声明；空 = 无镜像维度）。
    /// 每个候选 = { name, from, to }，`from` 是插件生成 URL 的官方前缀。
    fn mirrors(&self) -> &[MirrorCandidate] {
        &[]
    }
    /// Lua 插件：缺省加速镜像名（TOOL.default_mirror；缺省取 mirrors 中首个非官方候选，
    /// 无则 None = 不镜像）
    fn default_mirror(&self) -> Option<&str> {
        None
    }
    /// Lua 插件降级（P2）：返回 Lua 插件定义（非 Lua 插件返回 None）
    fn lua_def(&self) -> Option<&crate::lua_plugin::LuaPluginDef> {
        None
    }
    /// 工具 图标（插件声明部分）：Lua `TOOL.icon_base64` 的 base64 原文
    /// （可为 data URI）。仅负责声明透传；图标文件（icon.svg/png/jpg）由
    /// `tool::icon::resolve_icon` 在组装处统一解析（文件优先于声明）。
    fn icon_base64(&self) -> Option<&str> {
        None
    }
}

/// Tool trait（roadmap A：对应 vfox internal/tool/tool.go 的同步子集 + 文件操作）
/// 描述统一走 ToolDescriptor（Lua 插件实现）
pub trait Tool: Send + Sync {
    /// 描述抽象（Lua 插件统一入口）
    fn desc(&self) -> &dyn ToolDescriptor;

    fn name(&self) -> &str {
        self.desc().name()
    }

    fn display_name(&self) -> String {
        self.desc().display().to_string()
    }

    /// 已安装版本列表
    fn installed_versions(&self, paths: &PathMeta) -> Vec<Version> {
        install_mod::installed_versions(paths, self.name())
            .into_iter()
            .map(Version::new)
            .collect()
    }

    fn is_installed(&self, paths: &PathMeta, v: &Version) -> bool {
        paths.version_dir(self.name(), v.as_str()).exists()
    }

    /// 版本安装目录
    fn version_dir(&self, paths: &PathMeta, v: &Version) -> PathBuf {
        paths.version_dir(self.name(), v.as_str())
    }

    /// current 链接（Global scope）
    fn current_link(&self, paths: &PathMeta) -> PathBuf {
        paths.current_link(self.name())
    }

    /// current 链接指向的 bin 目录（加入 PATH 的路径）
    fn bin_dir(&self, paths: &PathMeta) -> PathBuf {
        let link = self.current_link(paths);
        if self.desc().bin_suffix().is_empty() {
            link
        } else {
            link.join(self.desc().bin_suffix().trim_start_matches('/'))
        }
    }

    /// 该版本对应的环境变量注入（PATH 指向链接而非真实目录，roadmap A：env_keys_for_scope）
    fn env_keys(&self, paths: &PathMeta, scope_active: bool) -> envhive_core::env::Envs {
        let mut envs = envhive_core::env::Envs::new();
        if scope_active {
            let bin = self.bin_dir(paths);
            envs.prepend_path(bin.to_string_lossy().to_string());
            for (k, kind) in self.desc().env_vars() {
                if matches!(kind, EnvVarKind::RootDir) {
                    let link = self.current_link(paths);
                    envs.var(*k, link.to_string_lossy().to_string());
                }
            }
        }
        envs
    }

    /// 插件附加环境变量（override；避免 Any downcast）
    fn extra_env_keys(&self, paths: &PathMeta, scope_active: bool) -> envhive_core::env::Envs {
        let _ = (paths, scope_active);
        envhive_core::env::Envs::new()
    }

    /// 重建 current 链接 → 指向某版本目录（roadmap C：切换 = 重建符号链接）
    fn rebuild_current_link(&self, paths: &PathMeta, v: &Version) -> Result<()> {
        let target = self.version_dir(paths, v);
        if !target.exists() {
            return Err(EnvHiveError::new(
                EnvHiveErrorKind::NotInstalled,
                format!("{} {} 未安装", self.name(), v.as_str()),
            ));
        }
        let link = self.current_link(paths);
        symlink::create_link(&target, &link)?;
        tracing::info!("[{}] current -> {}（{}）", self.name(), target.display(), v.as_str());
        Ok(())
    }

    /// 移除 current 链接
    fn remove_current_link(&self, paths: &PathMeta) -> Result<()> {
        symlink::remove_link(&self.current_link(paths))
    }

    /// current 链接当前指向的版本（读链接目标反推）
    fn current_version(&self, paths: &PathMeta) -> Option<Version> {
        let link = self.current_link(paths);
        let target = symlink::read_link(&link).ok()?;
        let name = target.file_name()?.to_string_lossy().to_string();
        name.strip_prefix("v-").map(|s| Version::new(s.to_string()))
    }
}

/// 工具 信息（前端卡片展示）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub display: String,
    pub category: String,
    pub homepage: String,
    pub current: Option<String>,
    pub installed: Vec<String>,
    /// 已缓存的可选版本列表（未拉取为 None）
    pub available: Option<Vec<String>>,
    /// 当前版本 bin 路径（PATH 中的条目，供展示）
    pub bin_path: Option<String>,
    /// Lua 插件：发行商维度（声明则前端渲染「发行商」下拉）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distributions: Option<Vec<DistributionInfo>>,
    /// Lua 插件：缺省发行商 key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_distribution: Option<String>,
    /// Lua 插件：下载加速镜像候选（声明则前端渲染「加速镜像」下拉，可切换）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirrors: Option<Vec<MirrorCandidate>>,
    /// Lua 插件：缺省加速镜像名
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_mirror: Option<String>,
    /// 工具 图标 data URI（`data:<mime>;base64,...`；无图标为 None，前端回退彩色圆点）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

impl ToolInfo {
    pub fn from_desc(
        desc: &dyn ToolDescriptor,
        paths: &PathMeta,
        chain: &envhive_core::toml_chain::ConfigChain,
        cached: Option<&runtime::VersionCache>,
    ) -> ToolInfo {
        let name = desc.name().to_string();
        let installed: Vec<String> = install_mod::installed_versions(paths, &name)
            .into_iter()
            .map(Version::new)
            .map(|v| v.as_str().to_string())
            .collect();
        let current = chain
            .tool(&name)
            .map(|t| t.version().to_string())
            .filter(|v| paths.version_dir(&name, v).exists())
            // 链接必须真实有效（junction/符号链接可解析）才显示"使用中"：
            // 若 current 只是空目录（创建失败残留），环境实际未生效，不显示"使用中"
            .filter(|_| envhive_core::env::symlink::read_link(&paths.current_link(&name)).is_ok());
        let available = cached.map(|c| c.version_strings());
        let bin_path = if current.is_some() {
            Some(paths.current_link(&name).join(desc.bin_suffix().trim_start_matches('/')).to_string_lossy().to_string())
        } else {
            None
        };
        let distributions = if desc.distributions().is_empty() {
            None
        } else {
            Some(desc.distributions().to_vec())
        };
        let default_distribution = desc.default_distribution().map(String::from);
        let mirrors = if desc.mirrors().is_empty() {
            None
        } else {
            Some(desc.mirrors().to_vec())
        };
        let default_mirror = desc.default_mirror().map(String::from);
        // 图标：插件目录文件（svg>png>jpg）优先，其次插件声明 base64
        let icon = icon::resolve_icon(&paths.plugins.join(&name), desc.icon_base64());
        ToolInfo {
            name,
            display: desc.display().to_string(),
            category: desc.category().to_string(),
            homepage: desc.homepage().to_string(),
            current,
            installed,
            available,
            bin_path,
            distributions,
            default_distribution,
            mirrors,
            default_mirror,
            icon,
        }
    }
}

/// 计算生效加速镜像名：显式声明的 `default_mirror` 优先，其次 mirrors 中首个非官方候选
/// （官方候选 = `from == to` 的占位项），无则 None（不镜像）。
pub fn effective_mirror_name(desc: &dyn ToolDescriptor) -> Option<String> {
    let mirrors = desc.mirrors();
    if mirrors.is_empty() {
        return None;
    }
    desc.default_mirror()
        .filter(|name| mirrors.iter().any(|m| m.name == *name))
        .map(String::from)
        .or_else(|| {
            mirrors
                .iter()
                .find(|m| m.from != m.to)
                .map(|m| m.name.clone())
        })
}

/// 已安装信息（首页展示）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledInfo {
    pub tool: String,
    pub display: String,
    pub version: String,
    pub path: String,
    pub is_current: bool,
}

/// 版本切换结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchResult {
    pub tool: String,
    pub version: String,
    pub bin_path: String,
    pub message: String,
}

/// 安装结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    pub tool: String,
    pub version: String,
    pub path: String,
    pub message: String,
}

pub type ToolMap = HashMap<String, Arc<dyn Tool>>;

/// 帮助函数：由全局配置链 + PathMeta 判断某 工具 是否已激活
pub fn is_active(chain: &envhive_core::toml_chain::ConfigChain, name: &str, paths: &PathMeta) -> bool {
    chain
        .tool(name)
        .map(|t| paths.version_dir(name, t.version()).exists())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Lua 插件：发行商维度辅助
// ---------------------------------------------------------------------------

/// 计算生效发行商：优先取前端请求值（须在 distributions 内），其次插件缺省，
/// 最后取 distributions[0]；无发行商维度的插件返回 None。
pub fn effective_distribution(desc: &dyn ToolDescriptor, requested: Option<&str>) -> Option<String> {
    let dists = desc.distributions();
    if dists.is_empty() {
        return None;
    }
    requested
        .filter(|k| dists.iter().any(|d| d.key == *k))
        .map(String::from)
        .or_else(|| desc.default_distribution().map(String::from))
        .or_else(|| dists.first().map(|d| d.key.clone()))
}

/// 版本缓存键：无发行商 → `<tool>`；有发行商 → `<tool>-<dist>`（天然隔离不同发行商/是否含 FX）
pub fn cache_key(tool: &str, distribution: Option<&str>) -> String {
    match distribution {
        Some(d) if !d.is_empty() => format!("{tool}-{d}"),
        _ => tool.to_string(),
    }
}
