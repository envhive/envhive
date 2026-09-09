//! P1 附加功能（roadmap 四/五章收尾）：
//! 1. 冲突检测（配置文件被外部修改 mtime/hash 指纹）
//! 2. 远程注册表（拉取插件 manifest → 下载安装）
//! 3. 环境导入 / 导出（YAML/JSON）

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::Digest;

use envhive_core::config::ProxyConfig;
use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::manager::EnvHiveManager;
use envhive_core::pathmeta::PathMeta;
use envhive_toolkit::plugin;
use envhive_toolkit::tool;

// ===========================================================================
// 1. 冲突检测：配置文件指纹（mtime + hash）
// ===========================================================================

/// 单个文件的指纹
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileFingerprint {
    pub mtime_ms: i64,
    pub hash: String,
}

/// 冲突信息（前端展示）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    /// "registry" | "tool"
    pub kind: String,
    pub tool: String,
    /// 冲突说明
    pub message: String,
    pub path: Option<String>,
    pub detail: Option<String>,
    /// tool 冲突：全局配置声明的版本（「重新切换」修复用；registry 冲突为 None）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// 指纹快照文件 `~/.envhive/state/fingerprints.json`
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FingerprintStore {
    /// tool -> 配置文件指纹（上次 apply 后记录）
    #[serde(default)]
    pub registry: HashMap<String, FileFingerprint>,
    /// 上次记录的指纹文件本身 mtime（防自身抖动）
    #[serde(default)]
    pub saved_at_ms: i64,
}

fn fingerprint_of(path: &std::path::Path) -> Option<FileFingerprint> {
    let meta = path.metadata().ok()?;
    let mtime = meta.modified().ok()?;
    let mtime_ms = mtime
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;
    let content = std::fs::read(path).ok()?;
    let hash = md5_hex(&content);
    Some(FileFingerprint { mtime_ms, hash })
}

fn md5_bytes(data: &[u8]) -> [u8; 16] {
    use md5::Md5;
    use sha2::Digest;
    let mut h = Md5::new();
    h.update(data);
    h.finalize().into()
}

fn md5_hex(data: &[u8]) -> String {
    md5_bytes(data).iter().map(|b| format!("{b:02x}")).collect()
}

fn fingerprints_file(paths: &PathMeta) -> PathBuf {
    paths.root.join("state").join("fingerprints.json")
}

fn load_fingerprints(paths: &PathMeta) -> FingerprintStore {
    let file = fingerprints_file(paths);
    std::fs::read_to_string(&file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_fingerprints(paths: &PathMeta, store: &FingerprintStore) {
    let file = fingerprints_file(paths);
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(store) {
        let _ = std::fs::write(&file, json);
    }
}

/// 记录某工具配置文件的当前指纹（apply 成功后调用）
pub fn record_registry_fingerprint(paths: &PathMeta, tool: &str, path: &std::path::Path) {
    let mut store = load_fingerprints(paths);
    match fingerprint_of(path) {
        Some(fp) => {
            store.registry.insert(tool.to_string(), fp);
            tracing::info!("[conflict] 已记录 {tool} 配置文件指纹 {}", path.display());
        }
        None => {
            store.registry.remove(tool);
            tracing::info!("[conflict] {tool} 配置文件不存在/不可读，清除指纹 {}", path.display());
        }
    }
    save_fingerprints(paths, &store);
}

/// 检测：某工具配置文件自上次应用后是否被外部修改（mtime + hash 双比对）
pub fn check_registry_conflicts(paths: &PathMeta, manager: &EnvHiveManager) -> Vec<ConflictInfo> {
    let store = load_fingerprints(paths);
    let mut out = Vec::new();
    for tool in manager.registry.supported_tools() {
        let Some(recorded) = store.registry.get(tool) else { continue };
        let Ok(path) = manager.registry.writer(tool).and_then(|w| w.config_path()) else { continue };
        if !path.exists() {
            out.push(ConflictInfo {
                kind: "registry".into(),
                tool: tool.to_string(),
                message: format!("{tool} 配置文件已被删除"),
                path: Some(path.to_string_lossy().to_string()),
                detail: None,
                version: None,
            });
            continue;
        }
        if let Some(cur) = fingerprint_of(&path) {
            if cur != *recorded {
                out.push(ConflictInfo {
                    kind: "registry".into(),
                    tool: tool.to_string(),
                    message: format!("{tool} 配置文件被外部修改（自上次应用后）"),
                    path: Some(path.to_string_lossy().to_string()),
                    detail: Some(format!("指纹变化: mtime {}ms vs {}ms，hash {} vs {}", cur.mtime_ms, recorded.mtime_ms, &cur.hash[..8], &recorded.hash[..8])),
                    version: None,
                });
            }
        }
    }
    out
}

/// 检测 工具 冲突：global 配置声明的版本与实际 current 链接不一致（切换中断 / 手工改动）
pub fn check_sdk_conflicts(paths: &PathMeta, manager: &EnvHiveManager) -> Vec<ConflictInfo> {
    let mut out = Vec::new();
    let chain = match manager.chain(None) {
        Ok(c) => c,
        Err(_) => return out,
    };
    for (name, version) in chain.active_tools() {
        let Ok(tool) = manager.lookup_tool(&name) else { continue };
        let dir = paths.version_dir(&name, &version);
        if !dir.exists() {
            out.push(ConflictInfo {
                kind: "tool".into(),
                tool: name.clone(),
                message: format!("{name}: 全局配置声明 {version} 但未安装"),
                path: Some(dir.to_string_lossy().to_string()),
                detail: Some("请先安装该版本，或使用“解除全局使用”".into()),
                version: Some(version.clone()),
            });
            continue;
        }
        let link_ver = tool.current_version(paths);
        if let Some(lv) = link_ver {
            if lv.as_str() != version {
                out.push(ConflictInfo {
                    kind: "tool".into(),
                    tool: name.clone(),
                    message: format!("{name}: 配置声明 {version}，但 current 链接指向 {}", lv.as_str()),
                    path: Some(paths.current_link(&name).to_string_lossy().to_string()),
                    detail: Some("可能是上次切换中断，点击「重新切换」即可修复".into()),
                    version: Some(version.clone()),
                });
            }
        }
    }
    out
}

/// 汇总全部冲突（registry + tool）
pub fn check_all_conflicts(paths: &PathMeta, manager: &EnvHiveManager) -> Vec<ConflictInfo> {
    let mut out = check_registry_conflicts(paths, manager);
    out.extend(check_sdk_conflicts(paths, manager));
    out
}

// ===========================================================================
// 2. 远程注册表（P1·开放）：manifest 拉取 + 插件安装（zip / 直链）
//    插件不再内置，全部从 Git 仓库（Gitee / GitHub raw 直链）下载：
//    - 仓库地址即 manifest.json 完整地址（当前官方仓库如 `{BASE}/plugins/manifest.json`，BASE 为仓库 raw 根）；
//    - 插件包：manifest 内 downloadUrl 为完整下载地址，或相对 manifest.json 所在目录的路径
//      （如 `zip/<name>.zip`）。
// ===========================================================================

/// 远程插件信息（manifest 条目；schema v2，向后兼容 v1）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePluginInfo {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// 插件包下载地址（manifest 内可为相对路径，如 `zip/go.zip`，拉取时按 manifest.json 所在目录解析）
    pub download_url: String,
    /// 插件类型："lua" | "toml"，缺省 toml（兼容 v1 manifest）
    #[serde(default)]
    pub r#type: String,
    /// 分发格式："zip" | "file"（直链文本），缺省 file（兼容 v1 manifest）
    #[serde(default)]
    pub format: String,
    /// zip 包 sha256 校验值（format=zip 时建议携带；不匹配拒绝安装）
    #[serde(default)]
    pub sha256: Option<String>,
    /// zip 包字节数（可选，供进度展示）
    #[serde(default)]
    pub size: Option<u64>,
    /// 插件主页（可选）
    #[serde(default)]
    pub homepage: String,
}

/// 远程注册表 manifest 结构：`{ "schemaVersion": 2, "plugins": [...] }`
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteManifest {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub plugins: Vec<RemotePluginInfo>,
}

/// manifest 内相对 downloadUrl（如 `plugins/zip/go.zip`）按 manifest.json 所在目录 base 解析为完整 URL
fn resolve_download_url(url: &mut String, base: &str) {
    if url.starts_with("http://") || url.starts_with("https://") {
        return;
    }
    *url = format!("{base}/{}", url.trim_start_matches('/'));
}

/// 计算候选 (manifest 完整 URL, 相对 downloadUrl 解析 base) 列表。
///
/// - 地址已以 `manifest.json` 结尾：唯一候选 = 该地址，base = 去掉文件名的目录；
/// - 否则视为仓库根（旧格式）：先试 `{base}/manifest.json`（历史仓库布局），
///   再试 `{base}/plugins/manifest.json`（当前官方仓库布局）。
fn manifest_candidates(address: &str) -> Vec<(String, String)> {
    let addr = address.trim().trim_end_matches('/');
    if addr.is_empty() {
        return Vec::new();
    }
    if addr.ends_with("manifest.json") {
        let base = addr.trim_end_matches("manifest.json").trim_end_matches('/').to_string();
        vec![(addr.to_string(), base)]
    } else {
        vec![
            (format!("{addr}/manifest.json"), addr.to_string()),
            (format!("{addr}/plugins/manifest.json"), addr.to_string()),
        ]
    }
}

/// 拉取远程插件 manifest。
///
/// `address` 为 **manifest.json 完整地址**（如 `https://raw.giteeusercontent.com/envhive/envhive/raw/main/plugins/manifest.json`），
/// 直接拉取；兼容旧格式的仓库根地址（无 manifest.json 后缀）时依次尝试 `{base}/manifest.json`、
/// `{base}/plugins/manifest.json`。相对 downloadUrl 一律按 manifest.json 所在目录解析。
pub async fn fetch_remote_manifest(address: &str, client: &reqwest::Client) -> Result<Vec<RemotePluginInfo>> {
    let candidates = manifest_candidates(address);
    let mut last_err: Option<EnvHiveError> = None;
    for (url, base) in candidates {
        match fetch_manifest_at(&url, &base, client).await {
            Ok(list) => return Ok(list),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        EnvHiveError::new(EnvHiveErrorKind::Network, format!("插件仓库 {address} 无可用 manifest"))
    }))
}

/// 拉取单个 manifest URL：GET → 解析 JSON → 相对 downloadUrl 按 base 解析
async fn fetch_manifest_at(
    url: &str,
    base: &str,
    client: &reqwest::Client,
) -> Result<Vec<RemotePluginInfo>> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Network, format!("拉取插件仓库 {url} 失败"), e))?;
    if !resp.status().is_success() {
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::Network,
            format!("插件仓库 {url} 返回 {}", resp.status()),
        ));
    }
    let mut manifest: RemoteManifest = resp
        .json()
        .await
        .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Network, "解析插件仓库 manifest 失败", e))?;
    for p in &mut manifest.plugins {
        resolve_download_url(&mut p.download_url, base);
    }
    Ok(manifest.plugins)
}

/// 安装远程插件：下载（zip 或直链文本）→ 校验 → 原子写入 `plugins/<name>/` → 写 `.market` 标记。
///
/// - `format=zip`（Lua 插件默认）：解压到 staging → 校验 `plugin.lua` 可加载 →
///   原子替换 `plugins/<name>/`（包内 icon.svg / lib/ 一并落盘）→ 写 `.market`；
/// - `format=file`：直链文本，按 `type=lua` 走 Lua 插件写入。
///
/// 同名插件已存在时视为「更新」：目录整体替换，已安装的工具版本目录保留、不卸载。
pub async fn install_remote_plugin(
    paths: &PathMeta,
    plugin: &RemotePluginInfo,
    client: &reqwest::Client,
) -> Result<()> {
    let name = &plugin.name;
    let is_zip = plugin.format.eq_ignore_ascii_case("zip");

    // 1. 下载到临时目录（staging 原子替换前置）
    let tmp_dir = paths.tmp.join(format!("plugin-dl-{name}-{}", std::process::id()));
    if tmp_dir.exists() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
    std::fs::create_dir_all(&tmp_dir)?;
    let download = async {
        let resp = client
            .get(&plugin.download_url)
            .send()
            .await
            .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Network, format!("下载插件 {name} 失败"), e))?;
        if !resp.status().is_success() {
            return Err(EnvHiveError::new(
                EnvHiveErrorKind::Network,
                format!("下载插件 {name} 返回 {}", resp.status()),
            ));
        }
        resp.bytes()
            .await
            .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Network, "读取插件内容失败", e))
    };
    let bytes = download.await?;

    // 2. sha256 完整性校验（manifest 携带时强制，防篡改 / 传输损坏）
    if let Some(expected) = &plugin.sha256 {
        let actual = hex::encode(sha2::Sha256::digest(&bytes));
        if !actual.eq_ignore_ascii_case(expected.trim()) {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err(EnvHiveError::new(
                EnvHiveErrorKind::Network,
                format!("插件 {name} sha256 校验失败（期望 {expected}，实际 {actual}），拒绝安装"),
            ));
        }
    }

    let result = if is_zip {
        let archive = tmp_dir.join(format!("{name}.zip"));
        std::fs::write(&archive, &bytes)?;
        install_remote_zip(paths, name, &archive)
    } else {
        let content = String::from_utf8_lossy(&bytes).to_string();
        if !plugin.r#type.eq_ignore_ascii_case("lua") {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err(EnvHiveError::new(
                EnvHiveErrorKind::Config,
                format!("插件 {name} 类型 {:?} 不受支持（仅支持 lua）", plugin.r#type),
            ));
        }
        envhive_toolkit::lua_plugin::add_lua_plugin(paths, name, &content)
    };
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(e);
    }

    // 3. 来源标记：远程安装/更新的插件写 `.market`（供「已安装插件」列表区分 market / local）
    let dir = plugin::plugin_dir(paths, name);
    if let Err(e) = std::fs::write(dir.join(".market"), "remote") {
        tracing::warn!("[插件安装] 写来源标记失败（不影响安装）: {e}");
    }

    let _ = std::fs::remove_dir_all(&tmp_dir);
    tracing::info!("[插件安装] {name}@{}{} 安装完成（{}）", plugin.version, plugin.r#type, plugin.download_url);
    Ok(())
}

/// zip 格式插件安装：解压 → 定位内容根 → 校验 plugin.lua 可加载 → 原子替换目标目录
fn install_remote_zip(paths: &PathMeta, name: &str, archive: &std::path::Path) -> Result<()> {
    let staging = archive
        .parent()
        .ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Io, "临时目录缺失"))?
        .join("staging");
    envhive_toolkit::tool::install::decompress_archive(archive, &staging, envhive_core::util::CompressKind::Zip)?;

    // 内容根：zip 根直接是插件文件（官方打包约定）；兼容顶层单目录包裹的 zip
    let root = resolve_plugin_content_root(&staging);
    let lua_file = root.join("plugin.lua");
    if !lua_file.exists() {
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::Config,
            format!("插件 {name} zip 包缺少 plugin.lua，拒绝安装"),
        ));
    }
    // 强校验：语法 + TOOL 元信息 + 必需 hook（available / pre_install）
    let script = std::fs::read_to_string(&lua_file).map_err(|e| {
        EnvHiveError::with_source(EnvHiveErrorKind::Io, format!("读取插件 {name} plugin.lua 失败"), e)
    })?;
    envhive_toolkit::lua_plugin::build_plugin_def(name, &script, &paths.root).map_err(|e| {
        EnvHiveError::with_source(
            EnvHiveErrorKind::Config,
            format!("插件 {name} 校验失败，拒绝安装"),
            e,
        )
    })?;

    // 原子替换：staging 内容根 → plugins/<name>/（util::replace_dir 内置 Defender 重试）
    let target = plugin::plugin_dir(paths, name);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    envhive_core::util::replace_dir(&root, &target).map_err(|e| {
        EnvHiveError::with_source(EnvHiveErrorKind::Io, format!("写入插件 {name} 目录失败"), e)
    })?;
    tracing::info!("[插件安装] {name} zip 解压校验通过 -> {}", target.display());
    Ok(())
}

/// 定位解压内容根：若 staging 下仅一个目录（且含 plugin.lua）则提升之；否则以 staging 为根。
fn resolve_plugin_content_root(staging: &std::path::Path) -> std::path::PathBuf {
    if staging.join("plugin.lua").exists() {
        return staging.to_path_buf();
    }
    let entries: Vec<_> = match std::fs::read_dir(staging) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => Vec::new(),
    };
    if entries.len() == 1 {
        let only = entries[0].path();
        if only.is_dir() && only.join("plugin.lua").exists() {
            return only;
        }
    }
    staging.to_path_buf()
}

/// 启动同步：从配置的插件仓库依次拉取 manifest，安装**缺失**的插件。
///
/// 替代旧版内置插件注入（`ensure_default_plugins`）：插件不再随二进制分发，
/// 首次启动从 Git 仓库拉取，保证开箱可用。规则：
/// - 已存在（含 `<name>.disabled` 禁用目录）的插件**不覆盖**（用户可能已修改）；
///   仅当目录缺少 `.market` 标记时补写（旧版内置插件来源迁移），不触碰插件文件；
/// - 首个可达仓库成功后不再尝试备用地址（Gitee 优先，GitHub 兜底）。
pub async fn sync_remote_plugins(paths: &PathMeta, addresses: &[String], client: &reqwest::Client) {
    for addr in addresses {
        let base = addr.trim().trim_end_matches('/');
        if base.is_empty() {
            continue;
        }
        let plugins = match fetch_remote_manifest(base, client).await {
            Ok(list) => list,
            Err(e) => {
                tracing::warn!("[插件同步] 拉取插件仓库 {base} 失败，尝试下一地址: {e}");
                continue;
            }
        };
        for p in &plugins {
            let dir = plugin::plugin_dir(paths, &p.name);
            let disabled = paths.plugins.join(format!("{}{}", p.name, plugin::DISABLED_SUFFIX));
            if dir.exists() || disabled.exists() {
                // 已存在 → 补 .market 来源标记（旧版内置插件迁移；不覆盖用户文件）
                let target = if dir.exists() { dir } else { disabled };
                if !target.join(".market").exists() {
                    if let Err(e) = std::fs::write(target.join(".market"), "remote") {
                        tracing::warn!("[插件同步] 补写 {}/.market 失败: {e}", p.name);
                    }
                }
                continue;
            }
            if let Err(e) = install_remote_plugin(paths, p, client).await {
                tracing::warn!("[插件同步] 安装 {}/{} 失败: {e}", p.name, p.version);
            }
        }
        break; // 仓库可达即停止（含 manifest 为空的情况）
    }
}

// ===========================================================================
// 3. 环境导出 / 导入（YAML / JSON）
// ===========================================================================

/// 导出环境快照（与 config 结构对应，可回导）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvSnapshot {
    /// 版本：快照格式版本
    pub version: u32,
    /// 全局工具版本：tool -> version
    #[serde(default)]
    pub tools: HashMap<String, String>,
    /// 镜像：tool -> preset 名
    #[serde(default)]
    pub registries: HashMap<String, String>,
    #[serde(default)]
    pub proxy: Option<ProxyConfig>,
    /// 环境变量
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// 导出当前环境为 YAML / JSON 字符串
pub fn export_env(manager: &EnvHiveManager, format: &str) -> Result<String> {
    let chain = manager.chain(None)?;
    let tools: HashMap<String, String> = chain
        .active_tools()
        .into_iter()
        .filter(|(name, _)| {
            // 只导出已安装版本，避免导出不完整状态
            paths_has_version(&manager.paths, name)
        })
        .collect();

    let mut registries = HashMap::new();
    for tool in manager.registry.supported_tools() {
        if let Ok(w) = manager.registry.writer(tool) {
            if let Ok(state) = w.read_current() {
                if let Some(p) = state.preset_name {
                    registries.insert(tool.to_string(), p);
                }
            }
        }
    }

    let proxy = manager.config.lock().unwrap().proxy.clone();
    let env = manager.config.lock().unwrap().env.clone();

    let snap = EnvSnapshot { version: 1, tools, registries, proxy: Some(proxy), env };

    match format.to_ascii_lowercase().as_str() {
        "json" => serde_json::to_string_pretty(&snap)
            .map_err(|e| EnvHiveError::new(EnvHiveErrorKind::Config, format!("导出 JSON 失败: {e}"))),
        _ => serde_yaml::to_string(&snap)
            .map_err(|e| EnvHiveError::new(EnvHiveErrorKind::Config, format!("导出 YAML 失败: {e}"))),
    }
}

fn paths_has_version(paths: &PathMeta, name: &str) -> bool {
    // 工具 全部为插件（Lua）：统一读已安装列表
    !envhive_toolkit::tool::install::installed_versions(paths, name).is_empty()
}

/// 导入环境快照（YAML 或 JSON 自动识别）：写 global tools + 镜像 + 代理 + env
pub fn import_env(manager: &EnvHiveManager, content: &str) -> Result<()> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(EnvHiveError::new(EnvHiveErrorKind::Config, "导入内容为空"));
    }
    // 自动识别：以 { 开头 → JSON，否则 YAML
    let snap: EnvSnapshot = if trimmed.starts_with('{') {
        serde_json::from_str(trimmed)
            .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Config, "解析导入 JSON 失败", e))?
    } else {
        serde_yaml::from_str(trimmed)
            .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Config, "解析导入 YAML 失败", e))?
    };

    if snap.version != 1 {
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::Config,
            format!("不支持的环境快照版本 {}（当前支持 1）", snap.version),
        ));
    }

    // 1. 全局工具版本（只写已安装的；未安装的跳过并警告）
    let mut chain = manager.chain(None)?;
    for (name, version) in &snap.tools {
        let tool = manager.lookup_tool(name)?;
        let v = tool::version::Version::new(version.clone());
        if !tool.is_installed(&manager.paths, &v) {
            tracing::warn!("[import] {name} {version} 未安装，跳过版本写入");
            continue;
        }
        let _ = v;
        chain.set_global_tool(name, envhive_core::toml_chain::ToolValue::plain(version.clone()))?;
        tool.rebuild_current_link(&manager.paths, &v)?;
    }
    manager.sync_user_env_pub(&chain)?;

    // 2. 镜像
    for (tool, preset) in &snap.registries {
        if let Err(e) = manager.apply_registry(tool, preset) {
            tracing::warn!("[import] 应用镜像 {tool}={preset} 失败: {e}");
        }
    }

    // 3. 代理
    if let Some(proxy) = &snap.proxy {
        manager.set_proxy(proxy.url.clone(), proxy.enable)?;
    }

    // 4. 环境变量（写入 config.yaml 全局 env）
    {
        let mut cfg = manager.config.lock().unwrap();
        cfg.env = snap.env.clone();
        cfg.save(&manager.paths.config_file)?;
    }
    tracing::info!("[import] 环境快照导入完成（{} 个 工具 / {} 个镜像 / {} 个环境变量）", snap.tools.len(), snap.registries.len(), snap.env.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use envhive_core::pathmeta;

    fn tmp_paths(tag: &str) -> (PathMeta, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("envhive-extras-{tag}-{}", std::process::id()));
        let paths = pathmeta::from_root(&root);
        std::fs::create_dir_all(&paths.plugins).unwrap();
        std::fs::create_dir_all(&paths.tmp).unwrap();
        (paths, root)
    }

    /// 仓库产物路径：`<repo>/plugins/zip/<name>.zip`（build_plugins.py 生成的真实产物）
    fn repo_zip(name: &str) -> std::path::PathBuf {
        // CARGO_MANIFEST_DIR = <repo>/app/src-tauri/crates/envhive-manager → 上溯 3 级到仓库根
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../plugins/zip").join(format!("{name}.zip"))
    }

    /// manifest 内相对 downloadUrl 按 manifest.json 所在目录解析为完整 URL（绝对 URL 保持原样）
    #[test]
    fn test_resolve_download_url() {
        let base = "https://raw.giteeusercontent.com/envhive/envhive/raw/main";
        let mut rel = "plugins/zip/go.zip".to_string();
        resolve_download_url(&mut rel, base);
        assert_eq!(rel, "https://raw.giteeusercontent.com/envhive/envhive/raw/main/plugins/zip/go.zip");

        let mut abs = "https://cdn.example.com/go.zip".to_string();
        resolve_download_url(&mut abs, base);
        assert_eq!(abs, "https://cdn.example.com/go.zip");
    }

    /// manifest.json 完整地址：唯一候选 + base 为去掉文件名的目录；仓库根地址兼容两种结构
    #[test]
    fn test_manifest_candidates() {
        // 新格式：manifest.json 完整地址
        let c = manifest_candidates("https://raw.githubusercontent.com/envhive/envhive/main/manifest.json");
        assert_eq!(
            c,
            vec![(
                "https://raw.githubusercontent.com/envhive/envhive/main/manifest.json".to_string(),
                "https://raw.githubusercontent.com/envhive/envhive/main".to_string(),
            )]
        );

        // 旧格式：仓库根 → 先根 manifest.json，再 plugins/manifest.json（base 均为仓库根）
        let c = manifest_candidates("https://raw.giteeusercontent.com/envhive/envhive/raw/main");
        assert_eq!(
            c,
            vec![
                ("https://raw.giteeusercontent.com/envhive/envhive/raw/main/manifest.json".to_string(), "https://raw.giteeusercontent.com/envhive/envhive/raw/main".to_string()),
                ("https://raw.giteeusercontent.com/envhive/envhive/raw/main/plugins/manifest.json".to_string(), "https://raw.giteeusercontent.com/envhive/envhive/raw/main".to_string()),
            ]
        );

        // 尾部斜杠 / 空串
        assert_eq!(manifest_candidates(""), Vec::<(String, String)>::new());
        assert_eq!(manifest_candidates("  ").len(), 0);
    }

    /// zip 安装链路：用仓库真实产物 plugins/zip/go.zip 走 install_remote_zip，
    /// 校验解压 → plugin.lua 强校验 → 原子写入 plugins/go/（plugin.lua + icon.svg）。
    #[test]
    fn test_install_remote_zip_from_repo_artifact() {
        let (paths, root) = tmp_paths("zipinstall");
        let archive = repo_zip("go");
        assert!(archive.exists(), "仓库产物缺失: {}", archive.display());

        install_remote_zip(&paths, "go", &archive).unwrap();

        // plugin.lua + icon.svg 已落盘
        assert!(paths.plugins.join("go/plugin.lua").exists(), "plugin.lua 应写入");
        assert!(paths.plugins.join("go/icon.svg").exists(), "icon.svg 应写入");

        // 可被宿主正常加载（语法 + TOOL + hooks）
        let def = envhive_toolkit::lua_plugin::load_lua_plugin(&paths, "go")
            .unwrap()
            .expect("安装后的 go 插件应可加载");
        assert_eq!(def.name, "go");
        assert_eq!(def.homepage, "https://go.dev");

        // 再次安装（更新语义）不报错、内容与首次一致
        let first = std::fs::read_to_string(paths.plugins.join("go/plugin.lua")).unwrap();
        install_remote_zip(&paths, "go", &archive).unwrap();
        assert_eq!(
            std::fs::read_to_string(paths.plugins.join("go/plugin.lua")).unwrap(),
            first,
            "更新安装后 plugin.lua 内容应保持一致"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 缺少 plugin.lua 的 zip 拒绝安装（弱校验在强校验前拦截）
    #[test]
    fn test_install_remote_zip_rejects_missing_plugin_lua() {
        let (paths, root) = tmp_paths("zipbad");
        let archive = paths.tmp.join("bad.zip");
        let f = std::fs::File::create(&archive).unwrap();
        let mut w = zip::ZipWriter::new(f);
        w.start_file("readme.txt", zip::write::SimpleFileOptions::default()).unwrap();
        use std::io::Write;
        w.write_all(b"not a plugin").unwrap();
        w.finish().unwrap();

        let err = install_remote_zip(&paths, "bad", &archive).unwrap_err();
        assert!(err.to_string().contains("plugin.lua"), "应提示缺少 plugin.lua，实际: {err}");
        assert!(!paths.plugins.join("bad").exists(), "失败不应残留插件目录");
        let _ = std::fs::remove_dir_all(&root);
    }
}
