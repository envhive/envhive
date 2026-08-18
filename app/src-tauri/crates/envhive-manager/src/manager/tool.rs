//! EnvHiveManager · 工具 域：查找 / 版本 / 安装 / 卸载 / 全局切换 / 列表查询
//!
//! 本模块包含 `EnvHiveManager` 的 工具 相关 impl 块与私有辅助函数
//! （`unix_now` / `is_expired` 供版本缓存 TTL 使用）。

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::AppHandle;

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::events::{self, DownloadStage};
use envhive_core::pathmeta::PathMeta;
use envhive_toolkit::plugin;
use envhive_toolkit::tool::download::{download_and_verify, Downloader};
use envhive_toolkit::tool::install::{self, InstallCtx};
use envhive_toolkit::tool::provider;
use envhive_toolkit::tool::resolver;
use envhive_toolkit::tool::runtime::VersionCache;
use envhive_toolkit::tool::{
    is_active, InstalledInfo, InstallResult, Tool, ToolDescriptor, ToolInfo, SwitchResult,
};
use envhive_core::toml_chain::{ConfigChain, ToolValue};
use envhive_core::util::CompressKind;

use super::{EnvHiveManager, VersionCacheEntry};

impl EnvHiveManager {
    // -----------------------------------------------------------------------
    // 工具 查找（Lua 插件）
    // -----------------------------------------------------------------------

    /// 工具 查找：Lua 插件；缓存实例
    pub fn lookup_tool(&self, name: &str) -> Result<Arc<dyn Tool>> {
        let key = name.to_ascii_lowercase();
        if let Some(tool) = self.open_tools.lock().unwrap().get(&key) {
            return Ok(tool.clone());
        }
        // Lua 插件
        if let Some(def) = envhive_toolkit::lua_plugin::load_lua_plugin(&self.paths, &key)? {
            let tool: Arc<dyn Tool> = Arc::new(envhive_toolkit::lua_plugin::LuaTool { plugin: Arc::new(def) });
            self.open_tools.lock().unwrap().insert(key, tool.clone());
            return Ok(tool);
        }
        Err(EnvHiveError::new(
            EnvHiveErrorKind::ToolNotFound,
            format!("工具 {name} 未注册（可在「插件」页添加 Lua 插件）"),
        ))
    }

    /// 工具 描述（内置 &'static / 插件运行时），供 provider 等使用
    pub fn lookup_desc(&self, name: &str) -> Result<Arc<dyn ToolDescriptor>> {
        let tool = self.lookup_tool(name)?;
        Ok(Arc::new(super::DescRef(tool)))
    }

    /// 版本缓存快照（search_versions 用；未拉取返回空缓存）
    /// 缓存 key 按发行商拆分（`<tool>-<dist>` / `<tool>`）
    pub fn version_cache_snapshot(&self, tool: &str, distribution: Option<&str>) -> VersionCache {
        let key = envhive_toolkit::tool::cache_key(tool, distribution);
        self.version_cache
            .lock()
            .unwrap()
            .get(&key)
            .map(|e| e.versions.clone())
            .unwrap_or_else(|| VersionCache { fetched_at: 0, versions: Vec::new() })
    }

    /// 全局配置链（含项目目录探测）
    pub fn chain(&self, project_dir: Option<&Path>) -> Result<ConfigChain> {
        ConfigChain::load(&self.paths, project_dir)
    }

    // -----------------------------------------------------------------------
    // 版本列表（roadmap E：拉取 + 缓存 TTL）
    // -----------------------------------------------------------------------

    /// 获取某 工具 的可用版本；`refresh=true` 强制刷新缓存。
    /// v2：`distribution` 为发行商 key（Lua 插件发行商维度；无发行商 工具 传 None，缺省解析见 effective_distribution）。
    pub async fn fetch_versions(
        &self,
        tool: &str,
        distribution: Option<&str>,
        refresh: bool,
    ) -> Result<VersionCache> {
        let desc = self.lookup_desc(tool)?;
        let dist = envhive_toolkit::tool::effective_distribution(desc.as_ref(), distribution);
        let cache_key = envhive_toolkit::tool::cache_key(desc.name(), dist.as_deref());
        let ttl = self.config.lock().unwrap().cache_ttl_secs();
        let cache_file = self.paths.versions_cache_file(desc.name(), dist.as_deref());

        if !refresh {
            if let Some(e) = self.version_cache.lock().unwrap().get(&cache_key) {
                if !is_expired(e.fetched_at, ttl) {
                    return Ok(e.versions.clone());
                }
            }
            if let Ok(raw) = std::fs::read_to_string(&cache_file) {
                if let Ok(cache) = serde_json::from_str::<VersionCache>(&raw) {
                    if !is_expired(cache.fetched_at, ttl) {
                        self.version_cache.lock().unwrap().insert(
                            cache_key.clone(),
                            VersionCacheEntry { fetched_at: cache.fetched_at, versions: cache.clone() },
                        );
                        return Ok(cache);
                    }
                }
            }
        }

        // 网络拉取（P2：可走下载加速镜像；用户自定义规则 + 插件声明的镜像候选）
        let client = self.client();
        let mirror = self.effective_mirror(desc.as_ref());
        let versions =
            provider::fetch_available(desc.provider(), desc.as_ref(), &client, Some(&mirror), dist.as_deref())
                .await?;
        if versions.is_empty() {
            return Err(EnvHiveError::new(
                EnvHiveErrorKind::Network,
                format!("{} 版本列表为空（检查网络或镜像源）", desc.display()),
            ));
        }
        let now = unix_now();
        let cache = VersionCache { fetched_at: now, versions };
        if let Some(parent) = cache_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&cache) {
            let _ = std::fs::write(&cache_file, json);
        }
        self.version_cache.lock().unwrap().insert(
            cache_key,
            VersionCacheEntry { fetched_at: now, versions: cache.clone() },
        );
        tracing::info!("[{}] 版本列表已刷新（{} 个版本）", desc.name(), cache.versions.len());
        Ok(cache)
    }

    /// 解析版本：先查已安装，再查可用列表（roadmap E：精确 → 前缀 → 标签）
    /// v2：`distribution` 决定拉取哪个发行商的版本列表（完整标识符精确匹配已安装时无需拉列表）
    pub async fn resolve_version(&self, tool: &str, query: &str, distribution: Option<&str>) -> Result<String> {
        let sdk_arc = self.lookup_tool(tool)?;
        let installed: Vec<String> = sdk_arc
            .installed_versions(&self.paths)
            .into_iter()
            .map(|v| v.as_str().to_string())
            .collect();
        if !installed.is_empty() {
            if let Ok(v) = resolver::resolve(&installed, query) {
                return Ok(v.to_string());
            }
        }
        let cache = self.fetch_versions(tool, distribution, false).await?;
        // 优先用带元数据的解析：lts / latest / stable 等标签依赖 AvailableVersion.lts / labels，
        // 已安装分支无元数据，匹配失败会自动回退到这里。
        resolver::resolve_versions_meta(&cache.versions, query).map(|s| s.to_string())
    }

    /// 全部 工具 名（Lua 插件）
    pub fn all_sdk_names(&self) -> Vec<String> {
        envhive_toolkit::plugin::list_plugins(&self.paths)
    }

    // -----------------------------------------------------------------------
    // 安装（roadmap D：Download → Checksum → Decompress → Verify）
    // -----------------------------------------------------------------------

    pub async fn install_tool(
        &self,
        app: &AppHandle,
        name: &str,
        query: &str,
        distribution: Option<&str>,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<InstallResult> {
        let result = self.install_sdk_inner(app, name, query, distribution, cancel).await;
        match &result {
            // 成功收尾：推送 Done 进度事件，前端据此清除顶部进度条 / 恢复按钮。
            // 修复：此前成功路径从不推送 Done（download 的 Done 只是"下载完成"，随后即被
            // Extracting 覆盖），导致顶部进度条永久停在「解压安装 100%」。
            Ok(_) => {
                events::emit_progress(
                    app,
                    &crate::events::DownloadProgress {
                        tool: name.to_string(),
                        version: query.to_string(),
                        percent: 100.0,
                        speed_mbps: 0.0,
                        stage: DownloadStage::Done,
                        url: None,
                        note: None,
                        total_bytes: None,
                        downloaded_bytes: 0,
                    },
                );
            }
            // 失败兜底：无论调用方是谁（队列 / 直接命令），都推送 Failed 进度事件 + 错误日志，
            // 保证前端进度条不会卡在 100%（用户报告的"解压安装卡住"问题）。
            // 注意：用户主动取消（Cancelled）不当作失败推送（前端按取消处理，无红色错误提示）。
            Err(e) => {
                if !matches!(e.kind, EnvHiveErrorKind::Cancelled) {
                    events::emit_progress(
                        app,
                        &crate::events::DownloadProgress {
                            tool: name.to_string(),
                            version: query.to_string(),
                            percent: 100.0,
                            speed_mbps: 0.0,
                            stage: DownloadStage::Failed,
                            url: None,
                            note: None,
                            total_bytes: None,
                            downloaded_bytes: 0,
                        },
                    );
                    events::emit_error(app, name, "INSTALL_FAILED", &e.to_string());
                }
                tracing::error!("[{name}] {query} 安装失败: {e}");
            }
        }
        result
    }

    async fn install_sdk_inner(
        &self,
        app: &AppHandle,
        name: &str,
        query: &str,
        distribution: Option<&str>,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<InstallResult> {
        let tool = self.lookup_tool(name)?;
        let desc = tool.desc();
        let sdk_name = desc.name().to_string();
        let display = desc.display().to_string();

        events::emit_install_status(app, &sdk_name, query, "resolving", Some("解析版本".into()));
        let version = self.resolve_version(&sdk_name, query, distribution).await?;

        // 已安装则直接返回
        let v = envhive_toolkit::tool::version::Version::new(version.clone());
        if tool.is_installed(&self.paths, &v) {
            let dir = tool.version_dir(&self.paths, &v);
            return Ok(InstallResult {
                tool: sdk_name.clone(),
                version,
                path: dir.to_string_lossy().to_string(),
                message: "该版本已安装".into(),
            });
        }

        // 包解析（平台映射 + URL；P2：下载加速镜像对 URL 前缀替换）
        events::emit_install_status(app, &sdk_name, &version, "resolving", Some("构造下载地址".into()));
        // Lua 插件的 pre_install 可能发起网络请求（http.get 走 blocking HTTP），
        // 必须放入 spawn_blocking 线程执行，避免在 tokio async 线程 panic 导致闪退。
        let dist_owned = distribution.map(String::from);
        // 镜像规则（用户自定义 + 插件声明）注入 pre_install，保证插件内请求（如 .sha256）同源
        let mirror_rules = self.effective_mirror_rules(desc);
        let mut pkg = match desc.lua_def() {
            Some(def) => {
                let def = def.clone();
                let v = version.clone();
                let d = dist_owned.clone();
                let rules = mirror_rules.clone();
                let r = tokio::task::spawn_blocking(move || {
                    envhive_toolkit::lua_plugin::resolve_lua_package(&def, &v, d.as_deref(), &rules)
                })
                    .await
                    .map_err(|e| {
                        EnvHiveError::with_source(
                            EnvHiveErrorKind::Internal,
                            format!("[{sdk_name}] pre_install 任务异常"),
                            e,
                        )
                    })?;
                r?
            }
            None => provider::resolve_package(desc, &version)?,
        };
        let mirror_cfg = self.effective_mirror(desc);
        let original_url = pkg.url.clone();
        pkg.url = envhive_toolkit::mirror::apply(Some(&mirror_cfg), &pkg.url);
        let mirrored = pkg.url != original_url;
        if mirrored {
            tracing::info!("[{sdk_name}] {version} 下载地址已镜像: {original_url} -> {}", pkg.url);
        }
        let kind = CompressKind::from_url(&pkg.url);
        if !kind.is_supported() {
            return Err(EnvHiveError::new(
                EnvHiveErrorKind::Unsupported,
                format!("暂不支持下载 {}（当前仅 zip / tar.gz / tar.xz）", pkg.url),
            ));
        }

        // 获取官方 checksum（尽力而为；镜像场景下 checksum 与下载同源）
        let client = self.client();
        let checksum = provider::fetch_checksum(desc, &version, &pkg, &client, Some(&mirror_cfg)).await?;
        if checksum.is_none() {
            tracing::warn!("[{sdk_name}] 未获取到官方 checksum，将跳过校验");
        }

        // 下载（断点续传 + 进度事件 + 可取消）。
        // 镜像下载失败（如镜像未同步该版本，404）→ 自动回退官方源重试一次，
        // 并通过 download-progress 的 note 提示前端（通知 + 队列消息体现）。
        let dest = self.paths.tool_cache_dir(&sdk_name).join(&pkg.file_name);
        let dl = Downloader {
            client: &client,
            progress: Some(&|p| events::emit_progress(app, p)),
            tool: &sdk_name,
            version: &version,
            cancel: cancel.clone(),
        };
        let archive = match download_and_verify(&dl, &pkg.url, &dest, checksum.as_deref()).await {
            Ok(f) => f,
            Err(e) if mirrored => {
                let msg = "镜像下载失败，已回退官方源".to_string();
                tracing::warn!("[{sdk_name}] {version} {msg}：{e}");
                events::emit_progress(
                    app,
                    &crate::events::DownloadProgress {
                        tool: sdk_name.clone(),
                        version: version.clone(),
                        percent: 0.0,
                        speed_mbps: 0.0,
                        stage: DownloadStage::Downloading,
                        url: Some(original_url.clone()),
                        note: Some(msg),
                        total_bytes: None,
                        downloaded_bytes: 0,
                    },
                );
                // 回退官方源重试（.part 若已有字节，官方源续传同发布文件兼容）
                pkg.url = original_url;
                download_and_verify(&dl, &pkg.url, &dest, checksum.as_deref()).await?
            }
            Err(e) => return Err(e),
        };

        // 下载完成但用户已取消：放弃安装（保留 .part 供续传），返回取消错误
        if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
            let _ = std::fs::remove_file(&archive);
            return Err(EnvHiveError::new(
                EnvHiveErrorKind::Cancelled,
                format!("[{sdk_name}] {version} 安装已取消"),
            ));
        }

        // 附加文件（单文件 工具 的伴随资源，如 Windows 上 exe 所需的 dll）：
        // 与主文件同一下载链路（镜像 + checksum + 进度事件），失败即安装失败
        let mut extra_paths: Vec<std::path::PathBuf> = Vec::new();
        for ef in &pkg.extra_files {
            let mut ef_url = ef.url.clone();
            ef_url = envhive_toolkit::mirror::apply(Some(&mirror_cfg), &ef_url);
            let ef_dest = self.paths.tool_cache_dir(&sdk_name).join(&ef.file_name);
            let dl = Downloader {
                client: &client,
                progress: Some(&|p| events::emit_progress(app, p)),
                tool: &sdk_name,
                version: &version,
                cancel: cancel.clone(),
            };
            let f = download_and_verify(&dl, &ef_url, &ef_dest, ef.checksum.as_deref()).await?;
            extra_paths.push(f);
        }

        // 解压 + 原子安装 + 验证（阻塞操作放 spawn_blocking）
        events::emit_install_status(app, &sdk_name, &version, "extracting", Some("解压并安装".into()));
        events::emit_progress(
            app,
            &crate::events::DownloadProgress {
                tool: sdk_name.clone(),
                version: version.clone(),
                percent: 100.0,
                speed_mbps: 0.0,
                stage: DownloadStage::Extracting,
                url: None,
                note: None,
                total_bytes: None,
                downloaded_bytes: 0,
            },
        );

        let paths_clone = self.paths.clone();
        let version_clone = version.clone();
        let archive_clone = archive.clone();
        let sdk_clone = tool.clone();
        let dist_clone = dist_owned.clone();
        let kind_clone = kind;
        let extra_clone = extra_paths.clone();
        // Lua 插件：pre_install 可动态指定 root_hint（解压根目录名；单文件安装不使用）
        let dynamic_hint = if let Some(def) = sdk_clone.desc().lua_def() {
            envhive_toolkit::lua_plugin::dynamic_root_hint(def)
        } else {
            None
        };
        let install_dir = tokio::task::spawn_blocking(move || {
            let desc_ref: &dyn ToolDescriptor = match dynamic_hint {
                Some(h) => &envhive_toolkit::tool::install::RootHintOverride { inner: sdk_clone.desc(), hint: h },
                None => sdk_clone.desc(),
            };
            let ctx = InstallCtx {
                paths: &paths_clone,
                desc: desc_ref,
                version: &version_clone,
                distribution: dist_clone.as_deref(),
            };
            if kind_clone.is_binary() {
                // 单文件可执行二进制（如 Lua）：直接放置安装，不解压
                install::install_binary(&ctx, &archive_clone, &extra_clone)
            } else {
                install::install_archive(&ctx, &archive_clone, kind_clone)
            }
        })
        .await
        .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Internal, "安装任务执行失败", e))??;

        // 清理下载的归档
        let _ = std::fs::remove_file(&archive);

        // P2：使用统计（安装也算一次使用）
        crate::usage::record_use(&self.paths, &sdk_name, &version, None);

        events::emit_install_status(app, &sdk_name, &version, "done", Some("安装完成".into()));
        tracing::info!("[{sdk_name}] {version} 安装完成");

        Ok(InstallResult {
            tool: sdk_name.clone(),
            version,
            path: install_dir.to_string_lossy().to_string(),
            message: format!("{display} 安装成功"),
        })
    }

    // -----------------------------------------------------------------------
    // 卸载
    // -----------------------------------------------------------------------

    pub fn uninstall_tool(&self, name: &str, version: &str) -> Result<()> {
        let tool = self.lookup_tool(name)?;
        let desc = tool.desc();
        let sdk_name = desc.name().to_string();

        let mut chain = self.chain(None)?;
        let is_current = chain.tool(&sdk_name).map(|t| t.version() == version).unwrap_or(false);
        if is_current {
            chain.clear_global_tool(&sdk_name)?;
            tool.remove_current_link(&self.paths)?;
            self.sync_user_env(&chain)?;
            tracing::info!("[{sdk_name}] 已解除全局使用 {version}");
        }

        // P2：Lua 插件卸载前钩子
        if let Some(def) = tool.desc().lua_def() {
            envhive_toolkit::lua_plugin::hook_pre_uninstall(def, version);
        }

        install::uninstall_version(&self.paths, desc, version)?;
        tracing::info!("[{sdk_name}] {version} 已卸载");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // 全局切换（roadmap C）
    // -----------------------------------------------------------------------

    pub async fn switch_global(&self, app: &AppHandle, name: &str, query: &str) -> Result<SwitchResult> {
        let tool = self.lookup_tool(name)?;
        let desc = tool.desc();
        let sdk_name = desc.name().to_string();

        let version = self.resolve_version(&sdk_name, query, None).await?;
        let v = envhive_toolkit::tool::version::Version::new(version.clone());
        if !tool.is_installed(&self.paths, &v) {
            return Err(EnvHiveError::new(
                EnvHiveErrorKind::NotInstalled,
                format!("{} {version} 尚未安装，请先安装再切换", desc.display()),
            ));
        }

        // 1. 写 Global scope TOML（先记录旧值，后续步骤失败时回滚，避免"显示使用中但环境未生效"）
        let mut chain = self.chain(None)?;
        let old_value = chain.tool(&sdk_name).cloned();
        chain.set_global_tool(&sdk_name, ToolValue::plain(version.clone()))?;

        // 2. 重建 current 链接（失败时回滚 TOML）
        if let Err(e) = tool.rebuild_current_link(&self.paths, &v) {
            tracing::error!("[{sdk_name}] 重建 current 链接失败，回滚全局配置: {e}");
            if let Err(rollback_err) = Self::restore_global_tool(&self.paths, &sdk_name, old_value.as_ref()) {
                tracing::error!("[{sdk_name}] 回滚全局配置失败: {rollback_err}");
            }
            return Err(e);
        }

        // 3. Windows：注册表 PATH + JAVA_HOME + 广播
        self.sync_user_env(&chain)?;

        events::emit_version_changed(app, &sdk_name, &version);
        tracing::info!("[{sdk_name}] 全局切换 -> {version}（{}）", tool.bin_dir(&self.paths).display());

        // P2：使用统计（切换记录）
        crate::usage::record_use(&self.paths, &sdk_name, &version, None);

        Ok(SwitchResult {
            tool: sdk_name.clone(),
            version,
            bin_path: tool.bin_dir(&self.paths).to_string_lossy().to_string(),
            message: "切换成功，新终端中生效".into(),
        })
    }

    /// 解除全局使用（不卸载）
    pub fn unuse_global(&self, name: &str) -> Result<()> {
        let tool = self.lookup_tool(name)?;
        let sdk_name = tool.desc().name().to_string();
        let mut chain = self.chain(None)?;
        let old_value = chain.tool(&sdk_name).cloned();
        chain.clear_global_tool(&sdk_name)?;
        if let Err(e) = tool.remove_current_link(&self.paths) {
            tracing::error!("[{sdk_name}] 移除 current 链接失败，回滚全局配置: {e}");
            if let Err(rollback_err) = Self::restore_global_tool(&self.paths, &sdk_name, old_value.as_ref()) {
                tracing::error!("[{sdk_name}] 回滚全局配置失败: {rollback_err}");
            }
            return Err(e);
        }
        self.sync_user_env(&chain)?;
        tracing::info!("[{sdk_name}] 已解除全局使用");
        Ok(())
    }

    /// 恢复 Global TOML 中某 工具 的配置值（失败回滚用；old 为 None 表示恢复为"未使用"）
    fn restore_global_tool(paths: &PathMeta, sdk_name: &str, old: Option<&ToolValue>) -> Result<()> {
        let mut chain = Self::chain_from(paths, None)?;
        match old {
            Some(v) => chain.set_global_tool(sdk_name, v.clone()),
            None => chain.clear_global_tool(sdk_name),
        }
    }

    /// 构造配置链（无 self 上下文，供回滚等静态场景使用）
    fn chain_from(paths: &PathMeta, project_dir: Option<&Path>) -> Result<ConfigChain> {
        ConfigChain::load(paths, project_dir)
    }

    // -----------------------------------------------------------------------
    // 查询
    // -----------------------------------------------------------------------

    /// 全部 工具 信息（Lua 插件）
    pub fn list_tools(&self, project_dir: Option<&Path>) -> Result<Vec<ToolInfo>> {
        let chain = self.chain(project_dir)?;
        let mut out = Vec::new();
        for name in plugin::list_plugins(&self.paths) {
            let Ok(tool) = self.lookup_tool(&name) else { continue };
            // v2：缓存按缺省发行商 key 读取（前端卡片初始展示；选择发行商后走 get_versions）
            let dist = envhive_toolkit::tool::effective_distribution(tool.desc(), None);
            let key = envhive_toolkit::tool::cache_key(&name, dist.as_deref());
            let cached = self.version_cache.lock().unwrap().get(&key).map(|e| e.versions.clone());
            out.push(ToolInfo::from_desc(tool.desc(), &self.paths, &chain, cached.as_ref()));
        }
        Ok(out)
    }

    /// 已安装列表（Lua 插件）
    pub fn list_installed(&self) -> Vec<InstalledInfo> {
        let chain = match self.chain(None) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut out = Vec::new();
        let mut push_sdk = |name: String, display: String| {
            let Ok(tool) = self.lookup_tool(&name) else { return };
            for v in tool.installed_versions(&self.paths) {
                let is_current = is_active(&chain, &name, &self.paths)
                    && chain.tool(&name).map(|t| t.version() == v.as_str()).unwrap_or(false);
                out.push(InstalledInfo {
                    tool: name.clone(),
                    display: display.clone(),
                    version: v.as_str().to_string(),
                    path: tool.version_dir(&self.paths, &v).to_string_lossy().to_string(),
                    is_current,
                });
            }
        };
        for name in plugin::list_plugins(&self.paths) {
            if let Ok(tool) = self.lookup_tool(&name) {
                push_sdk(name, tool.desc().display().to_string());
            }
        }
        out.sort_by(|a, b| a.display.cmp(&b.display).then(a.version.cmp(&b.version)));
        out
    }
}

fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn is_expired(fetched_at: i64, ttl: i64) -> bool {
    if ttl < 0 {
        return false;
    }
    if ttl == 0 {
        return true;
    }
    unix_now() - fetched_at >= ttl
}
