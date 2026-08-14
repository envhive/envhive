//! 版本发现源（roadmap E：四种 VersionProvider）
//! - NodeDist：nodejs.org dist API（Node.js）
//! - Adoptium：Adoptium API（Java / OpenJDK）
//! - GithubReleases：GitHub Releases API（Go）
//! - StaticIndex：静态文件索引（Rust）

use std::time::Duration;

use serde::Deserialize;

use envhive_core::bail;
use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::tool::runtime::{AvailableVersion, RuntimePackage};
use crate::tool::version::compare_versions;

/// Provider 类型（roadmap E / G：ToolDef.provider）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// nodejs.org dist API
    NodeDist,
    /// Adoptium API
    Adoptium,
    /// GitHub Releases API（需 owner/repo）
    GithubReleases,
    /// 静态索引（JSON 文件，键为版本号）
    StaticIndex,
    /// go.dev/dl JSON API（Go 官方版本列表，避免 GitHub 限流）
    GoDist,
    /// channel-rust-stable.toml（Rust 官方稳定发布清单）
    RustDist,
    /// 插件自定义 URL 模板（{version} {os} {arch} {ext} 占位符）
    UrlTemplate,
    /// Lua 插件（P2：available / pre_install hook 完全由脚本控制）
    Lua,
}

// ---------------------------------------------------------------------------
// 平台检测（roadmap D：std::env::consts::OS + ARCH → 平台别名映射）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformOs {
    Windows,
    Linux,
    Macos,
    #[allow(dead_code)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformArch {
    X64,
    Arm64,
    #[allow(dead_code)]
    Other,
}

pub fn current_platform() -> (PlatformOs, PlatformArch) {
    let os = match std::env::consts::OS {
        "windows" => PlatformOs::Windows,
        "linux" => PlatformOs::Linux,
        "macos" => PlatformOs::Macos,
        _ => PlatformOs::Other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" | "amd64" => PlatformArch::X64,
        "aarch64" | "arm64" => PlatformArch::Arm64,
        _ => PlatformArch::Other,
    };
    (os, arch)
}

// ---------------------------------------------------------------------------
// 版本列表抓取
// ---------------------------------------------------------------------------

/// 各 provider 的版本列表获取（roadmap E：从 工具 源拉取可用版本）
/// `mirror`：下载加速镜像（P2），Node/Rust 官方源 URL 会被重写到国内加速源。
/// `distribution`：v2 Lua 插件发行商 key（非 Lua provider 忽略）。
pub async fn fetch_available(
    kind: ProviderKind,
    desc: &dyn crate::tool::ToolDescriptor,
    client: &reqwest::Client,
    mirror: Option<&envhive_core::config::DownloadMirrorConfig>,
    distribution: Option<&str>,
) -> Result<Vec<AvailableVersion>> {
    match kind {
        ProviderKind::NodeDist => fetch_node_dist(client, mirror).await,
        ProviderKind::Adoptium => fetch_adoptium(client).await,
        ProviderKind::GithubReleases => {
            let (owner, repo) = desc
                .gh_repo()
                .ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Internal, "GithubReleases provider 缺少仓库定义"))?;
            fetch_github_releases(client, owner, repo).await
        }
        ProviderKind::StaticIndex | ProviderKind::UrlTemplate => {
            let url = desc
                .static_index_url()
                .ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Internal, "static_index provider 缺少 versions_url"))?;
            fetch_static_index(client, url).await
        }
        ProviderKind::GoDist => fetch_go_dist(client).await,
        ProviderKind::RustDist => fetch_rust_dist(client, mirror).await,
        ProviderKind::Lua => {
            // Lua hook 内可能发起网络请求（http.get 走 blocking HTTP）—— 若在 tokio
            // async 线程内直接调用会触发 "Cannot start a runtime from within a runtime"
            // panic 导致进程闪退，因此整体放入 spawn_blocking 线程执行。
            let def = match desc.lua_def() {
                Some(d) => d.clone(),
                None => {
                    return Err(EnvHiveError::new(
                        EnvHiveErrorKind::Internal,
                        format!("{} 声明为 Lua provider 但缺少插件定义", desc.name()),
                    ))
                }
            };
            let name = def.name.clone();
            let dist = crate::tool::effective_distribution(desc, distribution);
            // 镜像规则（from→to）注入插件 available() 内的 http.get（版本列表与下载同源）
            let mirror_rules: Vec<(String, String)> = mirror
                .map(|m| m.rules.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            let result = tokio::task::spawn_blocking(move || {
                crate::lua_plugin::hook_available(&def, dist.as_deref(), &mirror_rules)
            })
                .await
                .map_err(|e| {
                    EnvHiveError::with_source(
                        EnvHiveErrorKind::Internal,
                        format!("[{name}] available() 任务异常"),
                        e,
                    )
                })?;
            match result {
                Ok(versions) => Ok(versions),
                Err(e) => {
                    tracing::warn!("[{name}] Lua available hook 执行失败: {e}");
                    Ok(Vec::new())
                }
            }
        }
    }
}

async fn get_json(client: &reqwest::Client, url: &str) -> Result<serde_json::Value> {
    let resp = client
        .get(url)
        .timeout(Duration::from_secs(60))
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::Network,
            format!("请求 {url} 失败：HTTP {status}"),
        ));
    }
    resp.json::<serde_json::Value>().await.map_err(EnvHiveError::from)
}

/// Node.js：`https://nodejs.org/dist/index.json`（新版本在前；P2 可走 npmmirror 镜像）
async fn fetch_node_dist(client: &reqwest::Client, mirror: Option<&envhive_core::config::DownloadMirrorConfig>) -> Result<Vec<AvailableVersion>> {
    let url = crate::mirror::apply(mirror, "https://nodejs.org/dist/index.json");
    let v = get_json(client, &url).await?;
    let arr = v
        .as_array()
        .ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Network, "nodejs.org index.json 格式异常"))?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let raw = item.get("version").and_then(|x| x.as_str()).unwrap_or_default();
        let version = raw.trim_start_matches('v').to_string();
        if version.is_empty() {
            continue;
        }
        let lts = item.get("lts").map(|x| x.is_string()).unwrap_or(false);
        let mut labels = Vec::new();
        if lts {
            labels.push("lts".to_string());
        }
        out.push(AvailableVersion { version, lts, labels });
    }
    Ok(out)
}

/// Adoptium：`https://api.adoptium.net/v3/info/available_releases`（feature 版本号）
async fn fetch_adoptium(client: &reqwest::Client) -> Result<Vec<AvailableVersion>> {
    let v = get_json(client, "https://api.adoptium.net/v3/info/available_releases").await?;
    let lts_set: Vec<i64> = v
        .get("available_lts_releases")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|n| n.as_i64()).collect())
        .unwrap_or_default();
    let releases: Vec<i64> = v
        .get("available_releases")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|n| n.as_i64()).collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    for r in releases {
        let version = r.to_string();
        let lts = lts_set.contains(&r);
        let labels = if lts { vec!["lts".to_string()] } else { Vec::new() };
        out.push(AvailableVersion { version, lts, labels });
    }
    out.sort_by(|a, b| compare_versions(&a.version, &b.version));
    Ok(out)
}

/// GitHub Releases：`https://api.github.com/repos/{owner}/{repo}/releases`（新在前）
async fn fetch_github_releases(client: &reqwest::Client, owner: &str, repo: &str) -> Result<Vec<AvailableVersion>> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases?per_page=100");
    let v = get_json(client, &url).await?;
    let arr = v
        .as_array()
        .ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Network, "GitHub Releases 响应格式异常"))?;
    let mut out = Vec::new();
    for item in arr {
        let tag = item.get("tag_name").and_then(|x| x.as_str()).unwrap_or_default();
        let version = strip_tag_prefix(tag);
        if version.is_empty() {
            continue;
        }
        let stable = !version.contains('-') && !version.to_ascii_lowercase().contains("beta");
        let labels = if stable { vec!["stable".to_string()] } else { Vec::new() };
        out.push(AvailableVersion { version: version.to_string(), lts: false, labels });
    }
    Ok(out)
}

/// Go 官方：`https://go.dev/dl/?mode=json&include=all`（免 token，全量版本）
async fn fetch_go_dist(client: &reqwest::Client) -> Result<Vec<AvailableVersion>> {
    let v = get_json(client, "https://go.dev/dl/?mode=json&include=all").await?;
    let arr = v
        .as_array()
        .ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Network, "go.dev/dl 响应格式异常"))?;
    let mut out = Vec::new();
    for item in arr {
        let raw = item.get("version").and_then(|x| x.as_str()).unwrap_or_default();
        let version = strip_tag_prefix(raw);
        if version.is_empty() {
            continue;
        }
        let stable = item.get("stable").and_then(|x| x.as_bool()).unwrap_or(false);
        let labels = if stable { vec!["stable".to_string()] } else { Vec::new() };
        out.push(AvailableVersion { version: version.to_string(), lts: false, labels });
    }
    out.sort_by(|a, b| compare_versions(&a.version, &b.version));
    Ok(out)
}

/// Rust 官方：`channel-rust-stable.toml` 解析最新 stable（vfox rust 插件同款做法；P2 可走 rsproxy 镜像）
async fn fetch_rust_dist(client: &reqwest::Client, mirror: Option<&envhive_core::config::DownloadMirrorConfig>) -> Result<Vec<AvailableVersion>> {
    let url = crate::mirror::apply(mirror, "https://static.rust-lang.org/dist/channel-rust-stable.toml");
    let resp = client.get(&url).timeout(Duration::from_secs(60)).send().await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(EnvHiveError::new(EnvHiveErrorKind::Network, format!("请求 {url} 失败：HTTP {status}")));
    }
    let text = resp.text().await?;
    // [pkg.rust]\nversion = "1.97.1 (8bab26f4f 2026-07-14)"
    let marker = "[pkg.rust]";
    let idx = text.find(marker).ok_or_else(|| {
        EnvHiveError::new(EnvHiveErrorKind::Network, "channel-rust-stable.toml 缺少 [pkg.rust] 段")
    })?;
    let rest = &text[idx + marker.len()..];
    let line = rest
        .lines()
        .find(|l| l.trim_start().starts_with("version"))
        .ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Network, "channel-rust-stable.toml 缺少 version 字段"))?;
    let quoted = line.split('=').nth(1).unwrap_or("").trim().trim_matches('"');
    let version = quoted.split(' ').next().unwrap_or("").trim();
    if version.is_empty() {
        return Err(EnvHiveError::new(EnvHiveErrorKind::Network, "解析 rust 版本号失败"));
    }
    Ok(vec![AvailableVersion {
        version: version.to_string(),
        lts: false,
        labels: vec!["stable".to_string(), "latest".to_string()],
    }])
}

fn strip_tag_prefix(tag: &str) -> &str {
    // "go1.24.1" → "1.24.1"；"v1.2.3" → "1.2.3"；"1.2.3" 原样
    let lower = tag.to_ascii_lowercase();
    if lower.starts_with("go") && tag[2..].chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        &tag[2..]
    } else {
        tag.trim_start_matches(['v', 'V'])
    }
}

/// 静态索引：JSON 对象，键为版本号
async fn fetch_static_index(client: &reqwest::Client, url: &str) -> Result<Vec<AvailableVersion>> {
    let v = get_json(client, url).await?;
    let releases = v
        .get("releases")
        .and_then(|x| x.as_object())
        .ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Network, "静态索引缺少 releases 对象"))?;
    let mut out: Vec<AvailableVersion> = releases
        .keys()
        .map(|k| AvailableVersion { version: k.clone(), lts: false, labels: vec!["stable".to_string()] })
        .collect();
    out.sort_by(|a, b| compare_versions(&a.version, &b.version));
    Ok(out)
}

// ---------------------------------------------------------------------------
// 包地址解析（平台映射 + 下载 URL 模板）
// ---------------------------------------------------------------------------

/// 生成某 工具 某版本的下载包（roadmap D：平台检测 → 匹配正确二进制包）
pub fn resolve_package(desc: &dyn crate::tool::ToolDescriptor, version: &str) -> Result<RuntimePackage> {
    let (os, arch) = current_platform();
    let file_name;
    let url = match desc.provider() {
        ProviderKind::NodeDist => {
            let os_part = match os {
                PlatformOs::Windows => "win",
                PlatformOs::Linux => "linux",
                PlatformOs::Macos => "darwin",
                PlatformOs::Other => bail!(EnvHiveErrorKind::Unsupported, "Node.js 不支持当前操作系统"),
            };
            let arch_part = match arch {
                PlatformArch::X64 => "x64",
                PlatformArch::Arm64 => "arm64",
                PlatformArch::Other => bail!(EnvHiveErrorKind::Unsupported, "Node.js 不支持当前架构"),
            };
            let ext = if os == PlatformOs::Windows { "zip" } else { "tar.gz" };
            file_name = format!("node-v{version}-{os_part}-{arch_part}.{ext}");
            format!("https://nodejs.org/dist/v{version}/{file_name}")
        }
        ProviderKind::Adoptium => {
            let os_part = match os {
                PlatformOs::Windows => "windows",
                PlatformOs::Linux => "linux",
                PlatformOs::Macos => "mac",
                PlatformOs::Other => bail!(EnvHiveErrorKind::Unsupported, "Java 不支持当前操作系统"),
            };
            let arch_part = match arch {
                PlatformArch::X64 => "x64",
                PlatformArch::Arm64 => "aarch64",
                PlatformArch::Other => bail!(EnvHiveErrorKind::Unsupported, "Java 不支持当前架构"),
            };
            let ext = if os == PlatformOs::Windows { "zip" } else { "tar.gz" };
            file_name = format!("jdk-{version}-{os_part}-{arch_part}.{ext}");
            format!("https://api.adoptium.net/v3/binary/latest/{version}/ga/{os_part}/{arch_part}/jdk/hotspot/normal/eclipse")
        }
        ProviderKind::GithubReleases => {
            // 通用 GitHub Releases 下载（{owner}/{repo}/releases/download/{tag}/{file}）
            let (owner, repo) = desc
                .gh_repo()
                .ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Internal, "缺少 GitHub 仓库定义"))?;
            let os_part = match os {
                PlatformOs::Windows => "windows",
                PlatformOs::Linux => "linux",
                PlatformOs::Macos => "darwin",
                PlatformOs::Other => bail!(EnvHiveErrorKind::Unsupported, "当前操作系统不支持"),
            };
            let arch_part = match arch {
                PlatformArch::X64 => "amd64",
                PlatformArch::Arm64 => "arm64",
                PlatformArch::Other => bail!(EnvHiveErrorKind::Unsupported, "当前架构不支持"),
            };
            let ext = if os == PlatformOs::Windows { "zip" } else { "tar.gz" };
            file_name = format!("{}-{version}-{os_part}-{arch_part}.{ext}", desc.name());
            format!("https://github.com/{owner}/{repo}/releases/download/v{version}/{file_name}")
        }
        ProviderKind::GoDist => {
            // Go 二进制包由官方 dl.google.com 分发（GitHub Releases 无 assets）
            let os_part = match os {
                PlatformOs::Windows => "windows",
                PlatformOs::Linux => "linux",
                PlatformOs::Macos => "darwin",
                PlatformOs::Other => bail!(EnvHiveErrorKind::Unsupported, "Go 不支持当前操作系统"),
            };
            let arch_part = match arch {
                PlatformArch::X64 => "amd64",
                PlatformArch::Arm64 => "arm64",
                PlatformArch::Other => bail!(EnvHiveErrorKind::Unsupported, "Go 不支持当前架构"),
            };
            let ext = if os == PlatformOs::Windows { "zip" } else { "tar.gz" };
            file_name = format!("go{version}.{os_part}-{arch_part}.{ext}");
            format!("https://dl.google.com/go/{file_name}")
        }
        ProviderKind::StaticIndex | ProviderKind::RustDist => {
            let target = rust_target(os, arch)?;
            file_name = format!("rust-{version}-{target}.tar.gz");
            format!("https://static.rust-lang.org/dist/{file_name}")
        }
        ProviderKind::UrlTemplate => {
            // 插件自定义 URL 模板：{version} {os} {arch} {ext} 占位符
            let template = desc.url_template().ok_or_else(|| {
                EnvHiveError::new(EnvHiveErrorKind::Config, "url_template provider 缺少 url_template")
            })?;
            let (os_alias, arch_alias) = plugin_aliases(desc);
            let ext = if os == PlatformOs::Windows { "zip" } else { "tar.gz" };
            let url = template
                .replace("{version}", version)
                .replace("{os}", &os_alias)
                .replace("{arch}", &arch_alias)
                .replace("{ext}", ext);
            // 从 URL 尾部取文件名
            file_name = url.rsplit('/').next().unwrap_or("download").to_string();
            url
        }
        ProviderKind::Lua => {
            // Lua 插件：pre_install hook 决定下载地址（网络型 hook，调用方需在 spawn_blocking 中执行）
            // 安装链路中的发行商由 install_sdk_inner 通过 manager 直接走 resolve_lua_package 注入；
            // 此处为 Lua provider 的兜底路径（无发行商上下文）。
            let def = desc.lua_def().ok_or_else(|| {
                EnvHiveError::new(EnvHiveErrorKind::Internal, format!("{} 声明为 Lua provider 但缺少插件定义", desc.name()))
            })?;
            // 镜像规则：该兜底路径为同步调用（无镜像上下文），下载链路主路径
            // （install_sdk_inner 的 Lua 分支）已注入 effective_mirror_rules。
            return crate::lua_plugin::resolve_lua_package(def, version, None, &[]);
        }
    };
    Ok(RuntimePackage { tool: desc.name().to_string(), version: version.to_string(), url, checksum: None, file_name, extra_files: Vec::new() })
}

/// 插件自定义 URL 模板解析（{version} {os} {arch} {ext} 占位符）
/// 供 UrlTemplate provider 与 Lua 插件回退（无 pre_install）共用。
pub fn resolve_template_package(
    desc: &dyn crate::tool::ToolDescriptor,
    version: &str,
    template: &str,
) -> Result<RuntimePackage> {
    let (os, _arch) = current_platform();
    let (os_alias, arch_alias) = plugin_aliases(desc);
    let ext = if os == PlatformOs::Windows { "zip" } else { "tar.gz" };
    let url = template
        .replace("{version}", version)
        .replace("{os}", &os_alias)
        .replace("{arch}", &arch_alias)
        .replace("{ext}", ext);
    let file_name = url.rsplit('/').next().unwrap_or("download").to_string();
    Ok(RuntimePackage { tool: desc.name().to_string(), version: version.to_string(), url, checksum: None, file_name, extra_files: Vec::new() })
}

/// 插件平台别名解析（插件定义 platform 映射；无则用内置默认别名）
pub fn plugin_aliases(desc: &dyn crate::tool::ToolDescriptor) -> (String, String) {
    let (os, arch) = current_platform();
    let _os_name = match os {
        PlatformOs::Windows => "windows",
        PlatformOs::Linux => "linux",
        PlatformOs::Macos => "macos",
        PlatformOs::Other => "other",
    };
    let arch_name = match arch {
        PlatformArch::X64 => "x64",
        PlatformArch::Arm64 => "arm64",
        PlatformArch::Other => "other",
    };
    let default_os = match os {
        PlatformOs::Windows => "win",
        PlatformOs::Linux => "linux",
        PlatformOs::Macos => "darwin",
        PlatformOs::Other => "other",
    };
    // 优先使用插件平台映射
    if let Some(p) = desc.platform() {
        let pm = &p;
        let os_alias = if pm.os.is_empty() { default_os.to_string() } else { pm.os.clone() };
        let arch_alias = pm.arch.get(arch_name).cloned().unwrap_or_else(|| arch_name.to_string());
        return (os_alias, arch_alias);
    }
    // 无平台映射（内置 url_template 场景罕见）：回退 default
    (default_os.to_string(), arch_name.to_string())
}

fn rust_target(os: PlatformOs, arch: PlatformArch) -> Result<String> {
    match (os, arch) {
        (PlatformOs::Windows, PlatformArch::X64) => Ok("x86_64-pc-windows-msvc".into()),
        (PlatformOs::Windows, PlatformArch::Arm64) => Ok("aarch64-pc-windows-msvc".into()),
        (PlatformOs::Linux, PlatformArch::X64) => Ok("x86_64-unknown-linux-gnu".into()),
        (PlatformOs::Linux, PlatformArch::Arm64) => Ok("aarch64-unknown-linux-gnu".into()),
        (PlatformOs::Macos, PlatformArch::X64) => Ok("x86_64-apple-darwin".into()),
        (PlatformOs::Macos, PlatformArch::Arm64) => Ok("aarch64-apple-darwin".into()),
        _ => Err(EnvHiveError::new(EnvHiveErrorKind::Unsupported, "Rust 不支持当前平台")),
    }
}

// ---------------------------------------------------------------------------
// Checksum 获取（对照官方 checksum；拿不到则 "none" 跳过）
// ---------------------------------------------------------------------------

/// 获取某包的官方 checksum（`sha256:<hex>`）。无法获取返回 None（跳过校验并告警）。
/// `mirror`：P2 下载加速镜像 —— checksum URL 与下载 URL 保持同源。
pub async fn fetch_checksum(
    desc: &dyn crate::tool::ToolDescriptor,
    version: &str,
    pkg: &RuntimePackage,
    client: &reqwest::Client,
    mirror: Option<&envhive_core::config::DownloadMirrorConfig>,
) -> Result<Option<String>> {
    match desc.provider() {
        ProviderKind::NodeDist => {
            // SHASUMS256.txt 中找文件名对应行
            let sums_url = crate::mirror::apply(mirror, &format!("https://nodejs.org/dist/v{version}/SHASUMS256.txt"));
            let text = client.get(&sums_url).send().await?.error_for_status()?.text().await?;
            for line in text.lines() {
                let mut parts = line.split_whitespace();
                if let (Some(hash), Some(name)) = (parts.next(), parts.next()) {
                    if name == pkg.file_name && hash.len() == 64 {
                        return Ok(Some(format!("sha256:{hash}")));
                    }
                }
            }
            Ok(None)
        }
        ProviderKind::GithubReleases => {
            // 通用 GitHub 资产同名 .sha256（尽力而为）
            let (owner, repo) = desc
                .gh_repo()
                .ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Internal, "缺少 GitHub 仓库定义"))?;
            let sha_url = crate::mirror::apply(
                mirror,
                &format!(
                    "https://github.com/{owner}/{repo}/releases/download/v{version}/{}.sha256",
                    pkg.file_name
                ),
            );
            let resp = client.get(&sha_url).send().await?;
            if !resp.status().is_success() {
                return Ok(None);
            }
            let text = resp.text().await?;
            let hash = text.split_whitespace().next().unwrap_or_default().trim().to_string();
            if hash.len() == 64 {
                Ok(Some(format!("sha256:{hash}")))
            } else {
                Ok(None)
            }
        }
        ProviderKind::GoDist => {
            // Go 官方 checksum：dl.google.com 同名 .sha256 文件（P2 可走阿里云镜像）
            let sha_url = crate::mirror::apply(mirror, &format!("https://dl.google.com/go/{}.sha256", pkg.file_name));
            let resp = client.get(&sha_url).send().await?;
            if !resp.status().is_success() {
                return Ok(None);
            }
            let text = resp.text().await?;
            let hash = text.split_whitespace().next().unwrap_or_default().trim().to_string();
            if hash.len() == 64 {
                Ok(Some(format!("sha256:{hash}")))
            } else {
                Ok(None)
            }
        }
        ProviderKind::StaticIndex | ProviderKind::RustDist | ProviderKind::UrlTemplate | ProviderKind::Lua => {
            // {archive_url}.sha256（插件可附带 .sha256 文件）；pkg.url 已镜像过，幂等
            let sha_url = crate::mirror::apply(mirror, &format!("{}.sha256", pkg.url));
            let resp = client.get(&sha_url).send().await?;
            if !resp.status().is_success() {
                return Ok(None);
            }
            let text = resp.text().await?;
            let hash = text.split_whitespace().next().unwrap_or_default().trim().to_string();
            if hash.len() == 64 {
                Ok(Some(format!("sha256:{hash}")))
            } else {
                Ok(None)
            }
        }
        ProviderKind::Adoptium => {
            // 用不跟随重定向的请求读 X-Checksum-Sha256（Adoptium binary API 在 302 响应头携带）
            let no_redirect = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(EnvHiveError::from)?;
            let resp = no_redirect.get(&pkg.url).send().await?;
            if let Some(header) = resp.headers().get("X-Checksum-Sha256") {
                if let Ok(s) = header.to_str() {
                    return Ok(Some(format!("sha256:{}", s.trim().to_ascii_lowercase())));
                }
            }
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_tag() {
        assert_eq!(strip_tag_prefix("go1.24.1"), "1.24.1");
        assert_eq!(strip_tag_prefix("v2.1.0"), "2.1.0");
        assert_eq!(strip_tag_prefix("1.2.3"), "1.2.3");
    }
}
