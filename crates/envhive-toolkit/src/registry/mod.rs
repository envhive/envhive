//! 镜像源与仓库配置管理（roadmap H：蜂巢独有模块，"环境配置中枢"的核心差异化）
//!
//! 统一抽象 `ToolRegistryWriter`：读当前 → 备份 → 精确 merge → 原子写回 → 验证。
//! 内置工具：npm / pip / cargo / maven / go。

pub mod cargo;
pub mod conda;
pub mod docker;
pub mod gem;
pub mod go;
pub mod maven;
pub mod npm;
pub mod nuget;
pub mod proxy;
pub mod pubdev;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use envhive_core::util::safe_component;

/// 镜像预设（roadmap H：RegistryPreset）
/// 内置预设为静态数据（见下方常量表），自定义预设由 config.yaml 提供；
/// `apply` 前统一解析为 owned `RegistryPreset`（tool/name/url 为 String，extra 可空）。
#[derive(Debug, Clone)]
pub struct RegistryPreset {
    pub tool: String,
    pub name: String,
    pub url: String,
    /// 附加键值（trusted-host / sparse 标志等），按工具语义使用
    pub extra: Vec<(String, String)>,
}

impl RegistryPreset {
    /// 从静态表构建 owned 预设
    pub fn from_static(tool: &str, name: &str, url: &str, extra: &[(&str, &str)]) -> Self {
        RegistryPreset {
            tool: tool.to_string(),
            name: name.to_string(),
            url: url.to_string(),
            extra: extra.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }
}

/// 当前镜像状态
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryState {
    pub tool: String,
    pub current_url: Option<String>,
    /// 配置文件路径（不存在时为 None）
    pub config_file: Option<String>,
    /// 是否可识别为已知预设（用于 UI 高亮）
    pub preset_name: Option<String>,
}

/// 工具镜像写入器 trait（roadmap H）
pub trait ToolRegistryWriter: Send + Sync {
    fn tool_name(&self) -> &str;
    fn config_path(&self) -> Result<PathBuf>;
    /// 读取当前镜像配置（解析配置文件或执行查询命令）
    fn read_current(&self) -> Result<RegistryState>;
    /// 写入镜像 + 自动备份（备份文件 `<file>.envhive.bak.<ts>`）
    fn apply(&self, preset: &RegistryPreset) -> Result<()>;
    /// 运行官方命令验证生效（默认实现：读回比对 URL 含预设）
    fn verify(&self, preset: &RegistryPreset) -> Result<()> {
        let cur = self.read_current()?;
        let ok = cur
            .current_url
            .as_deref()
            .map(|u| u.contains(preset.url.as_str()) || preset.url.contains(u))
            .unwrap_or(false);
        if ok {
            Ok(())
        } else {
            Err(EnvHiveError::new(
                EnvHiveErrorKind::Registry,
                format!("{} 验证失败：当前 {cur:?} 未生效预设 {}", self.tool_name(), preset.name),
            ))
        }
    }
}

/// 通用：备份文件为 `<path>.envhive.bak.<unix_ts>`，返回备份路径
pub fn backup_file(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 注意：不能用 with_extension —— 它会「替换」原扩展名（如 config.toml → config.envhive.bak.<ts>），
    // 这里改为在完整路径「追加」后缀，保留原文件名（含扩展名），便于识别备份来源。
    let backup = PathBuf::from(format!("{}.envhive.bak.{}", path.to_string_lossy(), ts));
    std::fs::copy(path, &backup)?;
    tracing::info!("已备份 {} -> {}", path.display(), backup.display());
    Ok(Some(backup))
}

/// 校验工具名 / 预设名合法
pub fn ensure_name(kind: &str, name: &str) -> Result<()> {
    if !safe_component(name) {
        return Err(EnvHiveError::new(EnvHiveErrorKind::Config, format!("非法{kind}名称: {name:?}")));
    }
    Ok(())
}

/// 原子写回（临时文件 + rename），供各 writer 复用
pub(crate) fn atomic_write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// 预设是否带 "__clear__" 标记（docker/conda 的 official 预设 = 清空自定义项回默认）
pub(crate) fn preset_clears(preset: &RegistryPreset) -> bool {
    preset.extra.iter().any(|(k, _)| k.as_str() == "__clear__")
}

/// 提取 YAML 列表键的值（如 `:sources:` / `channels`）
pub(crate) fn extract_yaml_list(content: &str, key: &str) -> Option<Vec<String>> {
    let key_prefix = format!("{key}:");
    let mut in_list = false;
    let mut out = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(&key_prefix) {
            in_list = true;
            continue;
        }
        if in_list {
            if let Some(item) = trimmed.strip_prefix("- ") {
                out.push(item.trim().to_string());
            } else if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with('-') {
                // 下一个顶层键，列表结束
                in_list = false;
            }
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

/// 替换 YAML 列表键：`values` 为空则删除该键（回默认）；未找到键则追加
pub(crate) fn replace_yaml_list(content: &str, key: &str, values: &[String]) -> String {
    let key_prefix = format!("{key}:");
    let mut out: Vec<String> = Vec::new();
    let mut found = false;
    let mut in_list = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if in_list {
            // 列表项继续删除，直到非缩进行
            if line.starts_with(' ') || line.starts_with('\t') {
                if trimmed.starts_with("- ") || trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
            }
            if trimmed.starts_with("- ") {
                continue;
            }
            in_list = false;
        }
        if trimmed.starts_with(&key_prefix) {
            found = true;
            if values.is_empty() {
                continue; // 删除键（含其列表）
            }
            out.push(format!("{key}:"));
            for v in values {
                out.push(format!("  - {v}"));
            }
            in_list = true;
            continue;
        }
        out.push(line.to_string());
    }
    // 未找到键 → 追加（values 非空才追加）
    if !found && !values.is_empty() {
        if !out.is_empty() && !out.last().map(|l| l.is_empty()).unwrap_or(true) {
            out.push(String::new());
        }
        out.push(format!("{key}:"));
        for v in values {
            out.push(format!("  - {v}"));
        }
    }
    out.join("\n").trim_end().to_string() + "\n"
}

/// 提取 JSON 字符串键的值（简单实现，避免引完整解析）
pub(crate) fn extract_json_string(content: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let idx = content.find(&needle)?;
    let rest = &content[idx + needle.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let val = after.trim_start_matches('"');
    val.split('"').next().map(|s| s.to_string()).filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// 内置预设表（roadmap H：官方 ↔ 国内镜像）
// 元组：(tool, name, url, extra) —— 便于在 const 中使用；运行时统一构建为 owned。
// ---------------------------------------------------------------------------

type PresetTuple = (&'static str, &'static str, &'static str, &'static [(&'static str, &'static str)]);

pub const NPM_PRESETS: &[PresetTuple] = &[
    ("npm", "official", "https://registry.npmjs.org/", &[]),
    ("npm", "taobao", "https://registry.npmmirror.com/", &[("strict-ssl", "false")]),
    ("npm", "tencent", "https://mirrors.cloud.tencent.com/npm/", &[]),
    ("npm", "huawei", "https://repo.huaweicloud.com/repository/npm/", &[]),
];

pub const PIP_PRESETS: &[PresetTuple] = &[
    ("pip", "official", "https://pypi.org/simple", &[]),
    ("pip", "tuna", "https://pypi.tuna.tsinghua.edu.cn/simple", &[("trusted-host", "pypi.tuna.tsinghua.edu.cn")]),
    ("pip", "aliyun", "https://mirrors.aliyun.com/pypi/simple", &[("trusted-host", "mirrors.aliyun.com")]),
    ("pip", "tencent", "https://mirrors.cloud.tencent.com/pypi/simple", &[("trusted-host", "mirrors.cloud.tencent.com")]),
    ("pip", "huawei", "https://repo.huaweicloud.com/repository/pypi/simple", &[("trusted-host", "repo.huaweicloud.com")]),
];

pub const CARGO_PRESETS: &[PresetTuple] = &[
    ("cargo", "official", "https://github.com/rust-lang/crates.io-index", &[("sparse", "https://index.crates.io/")]),
    ("cargo", "rsproxy", "https://rsproxy.cn/index", &[("sparse", "sparse+https://rsproxy.cn/index/")]),
    ("cargo", "ustc", "https://mirrors.ustc.edu.cn/crates.io-index", &[("sparse", "sparse+https://mirrors.ustc.edu.cn/crates.io-index/")]),
];

pub const MAVEN_PRESETS: &[PresetTuple] = &[
    ("maven", "central", "https://repo1.maven.org/maven2", &[]),
    ("maven", "aliyun", "https://maven.aliyun.com/repository/central", &[]),
    ("maven", "huawei", "https://repo.huaweicloud.com/repository/maven", &[]),
    ("maven", "tencent", "https://mirrors.cloud.tencent.com/nexus/repository/maven-public/", &[]),
];

pub const GO_PRESETS: &[PresetTuple] = &[
    ("go", "official", "https://proxy.golang.org,direct", &[]),
    ("go", "goproxy.cn", "https://goproxy.cn,direct", &[]),
    ("go", "qiniu", "https://goproxy.io,direct", &[]),
];

// P2 · 增殖：镜像预设扩展（Docker / NuGet / RubyGems / pub.dev / conda）

/// docker：official = 清空 registry-mirrors（走 Docker Hub 默认），url 为占位说明
pub const DOCKER_PRESETS: &[PresetTuple] = &[
    ("docker", "official", "https://registry-1.docker.io", &[("__clear__", "1")]),
    ("docker", "ustc", "https://docker.mirrors.ustc.edu.cn", &[]),
    ("docker", "netease", "https://hub-mirror.c.163.com", &[]),
    ("docker", "baidu", "https://mirror.baidubce.com", &[]),
];

pub const NUGET_PRESETS: &[PresetTuple] = &[
    ("nuget", "official", "https://api.nuget.org/v3/index.json", &[]),
    ("nuget", "tuna", "https://mirrors.tuna.tsinghua.edu.cn/nuget/v3/index.json", &[]),
    ("nuget", "huawei", "https://repo.huaweicloud.com/repository/nuget/v3/index.json", &[]),
    ("nuget", "azurecn", "https://nuget.cdn.azure.cn/v3/index.json", &[]),
];

pub const GEM_PRESETS: &[PresetTuple] = &[
    ("gem", "official", "https://rubygems.org/", &[]),
    ("gem", "aliyun", "https://mirrors.aliyun.com/rubygems/", &[]),
    ("gem", "tuna", "https://mirrors.tuna.tsinghua.edu.cn/rubygems/", &[]),
];

pub const PUB_PRESETS: &[PresetTuple] = &[
    ("pub", "official", "https://pub.dev", &[]),
    ("pub", "sjtug", "https://mirrors.sjtug.sjtu.edu.cn/dart-pub", &[]),
    ("pub", "flutter-io", "https://pub.flutter-io.cn", &[]),
];

pub const CONDA_PRESETS: &[PresetTuple] = &[
    ("conda", "official", "https://repo.anaconda.com/pkgs/main", &[("__clear__", "1")]),
    ("conda", "tuna", "https://mirrors.tuna.tsinghua.edu.cn/anaconda/pkgs/main", &[("channel2", "https://mirrors.tuna.tsinghua.edu.cn/anaconda/pkgs/free")]),
    ("conda", "ustc", "https://mirrors.ustc.edu.cn/anaconda/pkgs/main", &[("channel2", "https://mirrors.ustc.edu.cn/anaconda/pkgs/free")]),
    ("conda", "aliyun", "https://mirrors.aliyun.com/anaconda/pkgs/main", &[("channel2", "https://mirrors.aliyun.com/anaconda/pkgs/free")]),
];

/// 全部内置预设表（按工具分组）
pub const ALL_BUILTIN_TABLES: &[&[PresetTuple]] = &[
    NPM_PRESETS,
    PIP_PRESETS,
    CARGO_PRESETS,
    MAVEN_PRESETS,
    GO_PRESETS,
    DOCKER_PRESETS,
    NUGET_PRESETS,
    GEM_PRESETS,
    PUB_PRESETS,
    CONDA_PRESETS,
];

/// 构建某工具的内置预设（owned）
pub fn builtin_presets(tool: &str) -> Vec<RegistryPreset> {
    ALL_BUILTIN_TABLES
        .iter()
        .flat_map(|t| t.iter())
        .filter(|(t, _, _, _)| *t == tool)
        .map(|(t, n, u, e)| RegistryPreset::from_static(t, n, u, e))
        .collect()
}

/// 全部内置预设（owned）
pub fn all_presets() -> Vec<RegistryPreset> {
    ALL_BUILTIN_TABLES
        .iter()
        .flat_map(|t| t.iter())
        .map(|(t, n, u, e)| RegistryPreset::from_static(t, n, u, e))
        .collect()
}

/// 按工具+预设名查找内置预设
pub fn find_preset(tool: &str, name: &str) -> Option<RegistryPreset> {
    builtin_presets(tool).into_iter().find(|p| p.name == name)
}

/// 按工具+预设名查找（内置优先，其次用户自定义源）
pub fn find_preset_with_customs(
    tool: &str,
    name: &str,
    customs: &[envhive_core::config::CustomRegistryPreset],
) -> Option<RegistryPreset> {
    if let Some(p) = find_preset(tool, name) {
        return Some(p);
    }
    customs
        .iter()
        .find(|c| c.tool == tool && c.name == name)
        .map(|c| RegistryPreset {
            tool: c.tool.clone(),
            name: c.name.clone(),
            url: c.url.clone(),
            extra: Vec::new(),
        })
}

/// 工具预设列表（供 UI）：内置 + 用户自定义
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetInfo {
    pub tool: String,
    pub name: String,
    pub url: String,
    pub is_official: bool,
    /// 是否为用户自定义源（UI 标记「自定义」）
    pub is_custom: bool,
}

pub fn presets_info(tool: &str, customs: &[envhive_core::config::CustomRegistryPreset]) -> Vec<PresetInfo> {
    let mut out: Vec<PresetInfo> = builtin_presets(tool)
        .into_iter()
        .map(|p| PresetInfo {
            tool: p.tool.clone(),
            name: p.name.clone(),
            url: p.url.clone(),
            is_official: p.name == "official" || p.name == "central",
            is_custom: false,
        })
        .collect();
    for c in customs.iter().filter(|c| c.tool == tool) {
        out.push(PresetInfo {
            tool: c.tool.clone(),
            name: c.name.clone(),
            url: c.url.clone(),
            is_official: false,
            is_custom: true,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// RegistryManager：按工具名路由 writer
// ---------------------------------------------------------------------------

pub struct RegistryManager {
    writers: HashMap<&'static str, Box<dyn ToolRegistryWriter>>,
}

impl RegistryManager {
    pub fn new() -> Self {
        let mut writers: HashMap<&'static str, Box<dyn ToolRegistryWriter>> = HashMap::new();
        writers.insert("npm", Box::new(npm::NpmWriter));
        writers.insert("pip", Box::new(npm::PipWriter));
        writers.insert("cargo", Box::new(cargo::CargoWriter));
        writers.insert("maven", Box::new(maven::MavenWriter));
        writers.insert("go", Box::new(go::GoWriter));
        // P2：镜像预设扩展
        writers.insert("docker", Box::new(docker::DockerWriter));
        writers.insert("nuget", Box::new(nuget::NugetWriter));
        writers.insert("gem", Box::new(gem::GemWriter));
        writers.insert("pub", Box::new(pubdev::PubWriter));
        writers.insert("conda", Box::new(conda::CondaWriter));
        RegistryManager { writers }
    }

    pub fn writer(&self, tool: &str) -> Result<&dyn ToolRegistryWriter> {
        self.writers
            .get(tool)
            .map(|b| b.as_ref())
            .ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::ToolNotFound, format!("不支持的镜像工具: {tool}")))
    }

    pub fn supported_tools(&self) -> Vec<&'static str> {
        vec!["npm", "pip", "cargo", "maven", "go", "docker", "nuget", "gem", "pub", "conda"]
    }

    /// 一键应用镜像（roadmap H：写入 + 自动备份）
    /// `customs`：用户自定义源（config.yaml），与内置预设合并解析
    pub fn apply(&self, tool: &str, preset_name: &str, customs: &[envhive_core::config::CustomRegistryPreset]) -> Result<()> {
        ensure_name("预设", preset_name)?;
        let preset = find_preset_with_customs(tool, preset_name, customs)
            .ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Config, format!("{tool} 无预设 {preset_name:?}")))?;
        let w = self.writer(tool)?;
        w.apply(&preset)?;
        tracing::info!("[{tool}] 镜像已切换 -> {} ({})", preset_name, preset.url);
        Ok(())
    }

    pub fn state(&self, tool: &str) -> Result<RegistryState> {
        self.writer(tool)?.read_current()
    }

    pub fn verify(&self, tool: &str, preset_name: &str, customs: &[envhive_core::config::CustomRegistryPreset]) -> Result<()> {
        let preset = find_preset_with_customs(tool, preset_name, customs)
            .ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Config, format!("{tool} 无预设 {preset_name:?}")))?;
        self.writer(tool)?.verify(&preset)
    }
}
