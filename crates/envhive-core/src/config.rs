//! 全局配置 `~/.envhive/config.yaml`（roadmap 模块 I，对应 vfox internal/config）
//! serde 反序列化 + 内置默认值 merge + 启动时 storage 路径写权限校验。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::util::expand_tilde;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppConfig {
    pub proxy: ProxyConfig,
    pub storage: StorageConfig,
    pub registry: RegistryConfig,
    pub cache: CacheConfig,
    /// 全局环境变量（进程注入 / Session 预览时并入，roadmap B：Envs.vars）
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    /// 下载加速镜像（P2：蜂巢自身 工具 下载的国内加速源）
    #[serde(default)]
    pub download_mirror: DownloadMirrorConfig,
    /// 开机自启动（P2：可选，常驻托盘）
    #[serde(default)]
    pub autostart: AutostartConfig,
    /// 用户自定义镜像源（网络页可追加：工具 + 名称 + URL，与内置预设合并展示）
    #[serde(default)]
    pub custom_registry: Vec<CustomRegistryPreset>,
}

/// 用户自定义镜像源（P2：网络页「添加自定义源」写入，持久化到 config.yaml）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomRegistryPreset {
    /// 目标工具（npm / pip / cargo / maven / go / docker / nuget / gem / pub / conda）
    pub tool: String,
    /// 预设名（下拉选项显示；同一工具内需唯一）
    pub name: String,
    /// 镜像地址（完整 URL）
    pub url: String,
}

/// 下载加速镜像配置（P2）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DownloadMirrorConfig {
    /// 总开关
    #[serde(default)]
    pub enable: bool,
    /// 用户自定义前缀替换规则：key = 被替换 URL 前缀，value = 新前缀。
    /// 优先级高于内置规则（内置：nodejs→npmmirror、go→阿里云、rust→rsproxy）。
    #[serde(default)]
    pub rules: std::collections::HashMap<String, String>,
}

/// 开机自启动 / 托盘常驻配置（P2，两个独立开关）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AutostartConfig {
    /// 开机自启动：开机时自动启动应用（携带 --autostart 参数，启动后隐藏主窗口）
    #[serde(default)]
    pub enable: bool,
    /// 托盘常驻：关闭窗口时隐藏到系统托盘，后台继续运行
    #[serde(default)]
    pub tray_resident: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfig {
    /// 代理地址，如 "http://127.0.0.1:7890"
    pub url: Option<String>,
    pub enable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageConfig {
    /// 工具 安装根目录（真实版本平铺于此），支持 `~`
    pub tool_path: String,
}

/// 远程插件仓库条目：name = 仓库名（插件市场下拉显示），url = manifest.json 完整地址
/// （如 `https://raw.githubusercontent.com/envhive/envhive/main/plugins/manifest.json`）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryEntry {
    /// 仓库名（插件市场下拉显示；如「官方gitee」「官方github」）
    pub name: String,
    /// manifest.json 完整地址（http/https；相对 downloadUrl 按该文件所在目录解析）
    pub url: String,
}

/// 远程插件仓库条目（兼容旧配置：纯字符串地址自动命名为「官方gitee/官方github/…」）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RegistryAddress {
    Named(RegistryEntry),
    Plain(String),
}

/// 默认远程插件仓库：Gitee raw 直链 `plugins/manifest.json`（国内优先，仓库名「官方gitee」）
pub const DEFAULT_REGISTRY_NAME: &str = "官方gitee";
pub const DEFAULT_REGISTRY_ADDRESS: &str =
    "https://raw.giteeusercontent.com/envhive/envhive/raw/main/plugins/manifest.json";
/// 备用远程插件仓库：GitHub raw 直链 `plugins/manifest.json`（仓库名「官方github」）
pub const DEFAULT_REGISTRY_NAME_FALLBACK: &str = "官方github";
pub const DEFAULT_REGISTRY_ADDRESS_FALLBACK: &str =
    "https://raw.githubusercontent.com/envhive/envhive/main/plugins/manifest.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryConfig {
    /// 远程插件仓库列表（多市场；第一个为默认/当前）。兼容旧配置的纯字符串地址数组。
    #[serde(default)]
    pub addresses: Vec<RegistryAddress>,
    /// 插件市场上次切换选中的仓库 URL（启动时默认展示；不在列表中时回退第一个）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<String>,
    /// 兼容旧配置：单地址字段（load 时迁移到 addresses 后清空，不再写回）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

/// 未配置名字的地址自动生成仓库名（Gitee/GitHub 识别官方名，其余取 host）
fn auto_registry_name(url: &str, index: usize) -> String {
    let lower = url.to_ascii_lowercase();
    if lower.contains("gitee") {
        DEFAULT_REGISTRY_NAME.to_string()
    } else if lower.contains("github") {
        DEFAULT_REGISTRY_NAME_FALLBACK.to_string()
    } else {
        let host = url
            .split("://")
            .nth(1)
            .and_then(|rest| rest.split(['/', '?', '#']).next())
            .filter(|h| !h.is_empty());
        match host {
            Some(h) => format!("{h} 仓库"),
            None => format!("仓库{}", index + 1),
        }
    }
}

impl RegistryConfig {
    /// 归一化的仓库条目列表：过滤空白/末尾斜杠 + 旧纯字符串自动命名 + 迁移旧 address 字段 + 兜底默认地址
    pub fn entries(&self) -> Vec<RegistryEntry> {
        let mut out: Vec<RegistryEntry> = Vec::new();
        for (i, a) in self.addresses.iter().enumerate() {
            let entry = match a {
                RegistryAddress::Named(e) => {
                    let url = e.url.trim().trim_end_matches('/').to_string();
                    if url.is_empty() {
                        continue;
                    }
                    RegistryEntry { name: e.name.trim().to_string(), url }
                }
                RegistryAddress::Plain(u) => {
                    let url = u.trim().trim_end_matches('/').to_string();
                    if url.is_empty() {
                        continue;
                    }
                    RegistryEntry { name: auto_registry_name(&url, i), url }
                }
            };
            if entry.name.is_empty() {
                out.push(RegistryEntry { name: auto_registry_name(&entry.url, i), url: entry.url });
            } else {
                out.push(entry);
            }
        }
        if out.is_empty() {
            if let Some(a) = self.address.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
                let url = a.trim_end_matches('/').to_string();
                out.push(RegistryEntry { name: auto_registry_name(&url, 0), url });
            }
        }
        if out.is_empty() {
            out.push(RegistryEntry {
                name: DEFAULT_REGISTRY_NAME.to_string(),
                url: DEFAULT_REGISTRY_ADDRESS.to_string(),
            });
        }
        out
    }

    /// 归一化的仓库地址列表（仅 URL，供同步/拉取使用）
    pub fn urls(&self) -> Vec<String> {
        self.entries().into_iter().map(|e| e.url).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheConfig {
    /// 版本列表缓存 TTL："12h" / "3600s" / "0"（禁用）/ "-1"（永不过期）
    pub available_hook_duration: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            proxy: ProxyConfig {
                url: Some("http://127.0.0.1:7890".into()),
                enable: false,
            },
            storage: StorageConfig { tool_path: "~/.envhive/installs".into() },
            registry: RegistryConfig {
                addresses: vec![
                    RegistryAddress::Named(RegistryEntry {
                        name: DEFAULT_REGISTRY_NAME.into(),
                        url: DEFAULT_REGISTRY_ADDRESS.into(),
                    }),
                    RegistryAddress::Named(RegistryEntry {
                        name: DEFAULT_REGISTRY_NAME_FALLBACK.into(),
                        url: DEFAULT_REGISTRY_ADDRESS_FALLBACK.into(),
                    }),
                ],
                selected: None,
                address: None,
            },
            cache: CacheConfig { available_hook_duration: "12h".into() },
            env: std::collections::HashMap::new(),
            download_mirror: DownloadMirrorConfig::default(),
            autostart: AutostartConfig::default(),
            custom_registry: Vec::new(),
        }
    }
}

impl AppConfig {
    /// 加载配置：文件不存在 → 默认值；存在 → serde_yaml 反序列化（缺字段用默认）。
    /// 解析失败时返回错误（调用方决定 fallback 到默认）。
    pub fn load(path: &Path) -> Result<AppConfig> {
        if !path.exists() {
            return Ok(AppConfig::default());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Io, format!("读取 {} 失败", path.display()), e))?;
        let mut cfg: AppConfig = serde_yaml::from_str(&content)?;
        // 迁移旧版单地址字段（address → addresses）
        if cfg.registry.addresses.is_empty() {
            if let Some(a) = cfg.registry.address.take() {
                let a = a.trim().trim_end_matches('/').to_string();
                if !a.is_empty() {
                    cfg.registry.addresses.push(RegistryAddress::Plain(a));
                }
            }
        } else {
            cfg.registry.address = None;
        }
        Ok(cfg)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_yaml::to_string(self)
            .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Config, "序列化 config.yaml 失败", e))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    /// 展开后的 工具 存储根目录（真实安装物）
    /// 兼容：旧版 config.yaml 持久化的路径可能仍含 `.envhive`（品牌迁移前），统一归一化为 `.envhive`。
    pub fn tool_root(&self) -> PathBuf {
        let p = expand_tilde(&self.storage.tool_path);
        let s = p.to_string_lossy();
        if s.contains(".envhive") {
            PathBuf::from(s.replace(".envhive", ".envhive"))
        } else {
            p
        }
    }

    /// 归一化的注册表地址列表：过滤空白/末尾斜杠 + 迁移旧 address 字段 + 兜底默认地址
    pub fn registry_addresses(&self) -> Vec<String> {
        self.registry.urls()
    }

    /// 校验存储根目录可写（roadmap I：启动时写权限校验，失败前端提示）
    pub fn verify_writable(&self) -> Result<()> {
        let root = self.tool_root();
        std::fs::create_dir_all(&root)?;
        let probe = root.join(".envhive-write-test");
        match std::fs::write(&probe, b"ok") {
            Ok(_) => {
                let _ = std::fs::remove_file(&probe);
                Ok(())
            }
            Err(e) => Err(EnvHiveError::with_source(
                EnvHiveErrorKind::Config,
                format!("工具 存储目录 {} 不可写", root.display()),
                e,
            )),
        }
    }

    /// 版本列表缓存 TTL（秒）；-1 = 永不过期；0 = 禁用缓存
    pub fn cache_ttl_secs(&self) -> i64 {
        parse_duration(&self.cache.available_hook_duration)
    }

    /// 全局环境变量（config.yaml 顶层 [env]）
    pub fn env_global(&self) -> &std::collections::HashMap<String, String> {
        &self.env
    }
}

/// 解析 "12h" / "30m" / "3600" / "-1" / "0"
fn parse_duration(s: &str) -> i64 {
    let t = s.trim().to_ascii_lowercase();
    if t == "-1" {
        return -1;
    }
    if let Ok(n) = t.parse::<i64>() {
        return n; // 纯数字按秒
    }
    let (num, unit) = t.split_at(t.len().saturating_sub(1));
    let Ok(n) = num.parse::<i64>() else { return 12 * 3600 };
    match unit {
        "h" => n * 3600,
        "m" => n * 60,
        "s" => n,
        "d" => n * 86400,
        _ => 12 * 3600,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 新配置（带名字的对象数组）反序列化 → entries 保留名字与地址
    #[test]
    fn test_registry_entries_named() {
        let yaml = r#"
registry:
  addresses:
    - name: 官方gitee
      url: https://raw.giteeusercontent.com/envhive/envhive/raw/main/plugins/manifest.json
    - name: 官方github
      url: https://raw.githubusercontent.com/envhive/envhive/main/plugins/manifest.json
"#;
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        let entries = cfg.registry.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "官方gitee");
        assert_eq!(entries[0].url, "https://raw.giteeusercontent.com/envhive/envhive/raw/main/plugins/manifest.json");
        assert_eq!(entries[1].name, "官方github");
        assert_eq!(entries[1].url, "https://raw.githubusercontent.com/envhive/envhive/main/plugins/manifest.json");
        assert_eq!(cfg.registry_addresses(), vec![
            "https://raw.giteeusercontent.com/envhive/envhive/raw/main/plugins/manifest.json",
            "https://raw.githubusercontent.com/envhive/envhive/main/plugins/manifest.json",
        ]);
    }

    /// 旧配置（纯字符串地址数组）反序列化 → 自动命名（gitee/github 识别官方名）
    #[test]
    fn test_registry_entries_legacy_plain() {
        let yaml = r#"
registry:
  addresses:
    - https://raw.giteeusercontent.com/envhive/envhive/raw/main
    - https://raw.githubusercontent.com/envhive/envhive/main
    - https://mirror.example.com/plugins/manifest.json
"#;
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        let entries = cfg.registry.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "官方gitee");
        assert_eq!(entries[1].name, "官方github");
        assert_eq!(entries[2].name, "mirror.example.com 仓库");
        // 旧地址（仓库根，无 manifest.json 后缀）保持原样，由拉取侧兼容解析
        assert_eq!(entries[2].url, "https://mirror.example.com/plugins/manifest.json");
    }

    /// 旧单 address 字段迁移 + 空列表兜底默认地址
    #[test]
    fn test_registry_entries_legacy_single_and_default() {
        let yaml = "registry:\n  address: https://example.com/repo\n";
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        let entries = cfg.registry.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].url, "https://example.com/repo");
        assert_eq!(entries[0].name, "example.com 仓库");

        let empty = AppConfig::default();
        let entries = empty.registry.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "官方gitee");
        assert_eq!(entries[0].url, DEFAULT_REGISTRY_ADDRESS);
        assert_eq!(entries[1].name, "官方github");
        assert_eq!(entries[1].url, DEFAULT_REGISTRY_ADDRESS_FALLBACK);
    }

    /// 序列化后 YAML 可回读（round-trip）
    #[test]
    fn test_registry_round_trip() {
        let cfg = AppConfig::default();
        let s = serde_yaml::to_string(&cfg).unwrap();
        let back: AppConfig = serde_yaml::from_str(&s).unwrap();
        assert_eq!(back.registry.entries(), cfg.registry.entries());
    }

    /// selected（插件市场上次切换的仓库）可持久化且 round-trip
    #[test]
    fn test_registry_selected_round_trip() {
        let yaml = r#"
registry:
  selected: https://raw.githubusercontent.com/envhive/envhive/main/plugins/manifest.json
  addresses:
    - name: 官方gitee
      url: https://raw.giteeusercontent.com/envhive/envhive/raw/main/plugins/manifest.json
"#;
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            cfg.registry.selected.as_deref(),
            Some("https://raw.githubusercontent.com/envhive/envhive/main/plugins/manifest.json")
        );
        let s = serde_yaml::to_string(&cfg).unwrap();
        let back: AppConfig = serde_yaml::from_str(&s).unwrap();
        assert_eq!(back.registry.selected, cfg.registry.selected);
    }
}
