//! EnvHiveManager（roadmap A：对应 vfox Manager，工具 实例缓存）
//! 职责：工具 查找缓存（Lua 插件）、版本解析、安装/卸载/全局切换编排、版本列表缓存、
//! Windows 注册表 PATH 同步、镜像源应用、环境预览与进程注入。
//!
//! 按职责拆分为子模块（同一 `EnvHiveManager` 类型的多个 impl 块）：
//! - `tool`：工具 查找 / 版本 / 安装 / 卸载 / 全局切换 / 列表查询
//! - `env`：环境解析、进程注入、系统环境变量应用
//! - `registry`：镜像源（registry）与代理
//! - `plugin`：插件管理（Lua）
//! - `mirror`：下载加速镜像
//! - `prefs`：开机自启动 / 托盘常驻 / 项目预设

mod env;
mod mirror;
mod plugin;
mod prefs;
mod registry;
mod tool;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use envhive_core::config::AppConfig;
use envhive_core::pathmeta::PathMeta;
use envhive_toolkit::registry::RegistryManager;
use envhive_toolkit::tool::provider;
use envhive_toolkit::tool::{Tool, ToolDescriptor};

/// 版本列表缓存条目
struct VersionCacheEntry {
    fetched_at: i64,
    versions: envhive_toolkit::tool::runtime::VersionCache,
}

pub struct EnvHiveManager {
    /// 全局配置（Mutex：支持运行时改代理）
    pub config: Mutex<AppConfig>,
    pub paths: PathMeta,
    /// 动态 client（proxy 开关变化时重建）
    client: Mutex<reqwest::Client>,
    /// name(小写) -> Tool 实例（roadmap A：open_tools 缓存）
    open_tools: Mutex<HashMap<String, Arc<dyn Tool>>>,
    /// 版本缓存：tool -> 缓存条目
    version_cache: Mutex<HashMap<String, VersionCacheEntry>>,
    /// 镜像源管理器（roadmap H）
    pub registry: RegistryManager,
}

impl EnvHiveManager {
    pub fn new(config: AppConfig, paths: PathMeta, client: reqwest::Client) -> Self {
        // 清理旧版（第 3 层）写入用户环境变量的代理残留（升级后一次性清理）
        let _ = envhive_toolkit::registry::proxy::cleanup_legacy_env_vars(&config.proxy);
        // 插件内 http.get 与宿主下载共享同一代理配置（应用内生效）
        envhive_toolkit::lua_plugin::set_plugin_proxy(if config.proxy.enable { config.proxy.url.as_deref() } else { None });
        // 注意：不再注入内置插件 —— 插件全部来自 Git 仓库（manifest + zip），
        // 启动后台同步在 lib.rs::run 的 setup 阶段触发（extras::sync_remote_plugins）。
        EnvHiveManager {
            config: Mutex::new(config),
            paths,
            client: Mutex::new(client),
            open_tools: Mutex::new(HashMap::new()),
            version_cache: Mutex::new(HashMap::new()),
            registry: RegistryManager::new(),
        }
    }

    /// 当前 HTTP client（代理可能已重建）
    pub fn client(&self) -> reqwest::Client {
        self.client.lock().unwrap().clone()
    }

    /// 工具 查找器适配（供 toolkit::env_resolver 复用缓存）
    /// `EnvHiveManager` 的 `open_tools` 缓存与 `ToolLookup` 结构相同，直接桥接。
    fn lookup(&self) -> envhive_toolkit::env_resolver::ToolLookup {
        use envhive_toolkit::env_resolver::ToolLookup;
        let mut cache = std::collections::HashMap::new();
        for (k, v) in self.open_tools.lock().unwrap().iter() {
            cache.insert(k.clone(), v.clone());
        }
        // 预填充缓存：后续 lookup 命中已打开的 工具，避免重复加载插件
        let lookup = ToolLookup::new();
        for (k, v) in cache {
            lookup.seed(&k, v);
        }
        lookup
    }

    /// 按代理配置重建 client（roadmap H：蜂巢下载代理；插件 http.get 同步同配置）
    pub fn rebuild_client(&self) {
        let cfg = self.config.lock().unwrap().proxy.clone();
        envhive_toolkit::lua_plugin::set_plugin_proxy(if cfg.enable { cfg.url.as_deref() } else { None });
        let mut builder = reqwest::Client::builder()
            .user_agent("envhive/0.1.0")
            .connect_timeout(std::time::Duration::from_secs(15));
        if cfg.enable {
            if let Some(url) = &cfg.url {
                if let Ok(proxy) = reqwest::Proxy::all(url) {
                    builder = builder.proxy(proxy);
                }
            }
        }
        *self.client.lock().unwrap() = builder.build().unwrap_or_default();
        tracing::info!("HTTP client 已按代理配置重建（enable={}）", cfg.enable);
    }
}

/// 应用为系统环境变量结果
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyGlobalResult {
    pub applied: bool,
    pub message: String,
}

/// 轻量 desc 引用（lookup_desc 返回用）
struct DescRef(Arc<dyn Tool>);
impl ToolDescriptor for DescRef {
    fn name(&self) -> &str { self.0.desc().name() }
    fn display(&self) -> &str { self.0.desc().display() }
    fn category(&self) -> &str { self.0.desc().category() }
    fn homepage(&self) -> &str { self.0.desc().homepage() }
    fn provider(&self) -> provider::ProviderKind { self.0.desc().provider() }
    fn gh_repo(&self) -> Option<(&str, &str)> { self.0.desc().gh_repo() }
    fn static_index_url(&self) -> Option<&str> { self.0.desc().static_index_url() }
    fn url_template(&self) -> Option<&str> { self.0.desc().url_template() }
    fn platform(&self) -> Option<&envhive_toolkit::tool::PlatformMap> { self.0.desc().platform() }
    fn verify_bin(&self) -> &str { self.0.desc().verify_bin() }
    fn verify_arg(&self) -> &str { self.0.desc().verify_arg() }
    fn bin_suffix(&self) -> &str { self.0.desc().bin_suffix() }
    fn env_vars(&self) -> &[(&str, envhive_toolkit::tool::EnvVarKind)] { self.0.desc().env_vars() }
    fn root_hint(&self) -> Option<&str> { self.0.desc().root_hint() }
    // 透传发行商维度与 Lua 插件定义（fetch_versions / effective_distribution 依赖）
    fn distributions(&self) -> &[envhive_toolkit::tool::DistributionInfo] { self.0.desc().distributions() }
    fn default_distribution(&self) -> Option<&str> { self.0.desc().default_distribution() }
    fn lua_def(&self) -> Option<&envhive_toolkit::lua_plugin::LuaPluginDef> { self.0.desc().lua_def() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envhive_core::pathmeta;

    /// 读取仓库插件源码 fixture：`<repo>/plugins/src/<name>/plugin.lua`
    /// （插件不再内置，源码统一维护在仓库 plugins/src/ 下）
    fn fixture_script(name: &str) -> String {
        // CARGO_MANIFEST_DIR = <repo>/app/src-tauri/crates/envhive-manager → 上溯 4 级到仓库根
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../plugins/src");
        std::fs::read_to_string(base.join(name).join("plugin.lua"))
            .unwrap_or_else(|e| panic!("fixture 插件 {name} 缺失: {e}"))
    }

    /// 无内置插件：EnvHiveManager 构造不再注入任何插件（插件全部来自 Git 仓库）；
    /// 手工添加仓库 fixture 插件后可列出、可查找、修改不被覆盖（无网络依赖）。
    #[test]
    fn test_no_builtin_plugins_and_manual_add_works() {
        let root = std::env::temp_dir().join(format!("envhive-mgr-{}", std::process::id()));
        let paths = pathmeta::from_root(&root);
        std::fs::create_dir_all(&paths.plugins).unwrap();
        let mgr = EnvHiveManager::new(AppConfig::default(), paths.clone(), reqwest::Client::new());

        // 1. 构造后无任何插件（内置插件已移除）
        assert!(
            mgr.all_sdk_names().is_empty(),
            "应无内置插件注入，实际: {:?}",
            mgr.all_sdk_names()
        );

        // 2. 手工安装仓库插件（等价于市场安装）→ 可列出、可查找
        envhive_toolkit::lua_plugin::add_lua_plugin(&paths, "nodejs", &fixture_script("nodejs")).unwrap();
        let names = mgr.all_sdk_names();
        assert!(names.contains(&"nodejs".to_string()), "手工添加后应存在，实际: {names:?}");
        let tool = mgr.lookup_tool("nodejs").unwrap();
        assert_eq!(tool.desc().verify_bin(), "node");

        // 3. 已安装插件不被覆盖：修改后再次构造 manager，脚本保持不变
        let file = envhive_toolkit::lua_plugin::plugin_file(&paths, "nodejs");
        std::fs::write(&file, "-- user edited\n").unwrap();
        let _mgr2 = EnvHiveManager::new(AppConfig::default(), paths.clone(), reqwest::Client::new());
        assert!(
            std::fs::read_to_string(&file).unwrap().starts_with("-- user edited"),
            "重建 manager 不应覆盖用户已有插件"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 禁用/启用链路：toggle_plugin 重命名目录 → 列表仍显示（enabled=false）但 list_tools 排除；
    /// 重新启用后恢复；禁用插件不会被重新注入。
    #[test]
    fn test_toggle_plugin_disable_enable() {
        let root = std::env::temp_dir().join(format!("envhive-mgr-toggle-{}", std::process::id()));
        let paths = pathmeta::from_root(&root);
        std::fs::create_dir_all(&paths.plugins).unwrap();
        let mgr = EnvHiveManager::new(AppConfig::default(), paths.clone(), reqwest::Client::new());
        envhive_toolkit::lua_plugin::add_lua_plugin(&paths, "nodejs", &fixture_script("nodejs")).unwrap();

        // 1. 禁用 nodejs
        mgr.toggle_plugin("nodejs", false).unwrap();
        assert!(!mgr.open_tools.lock().unwrap().contains_key("nodejs"), "禁用后应从缓存移除");
        assert!(paths.plugins.join("nodejs.disabled").join("plugin.lua").exists());
        assert!(!paths.plugins.join("nodejs").join("plugin.lua").exists());

        // 2. 列表仍显示 nodejs（enabled=false），list_tools 不再包含
        let infos = mgr.list_plugins();
        let n = infos.iter().find(|p| p.name == "nodejs").expect("禁用插件应仍显示在列表");
        assert!(!n.enabled, "禁用插件 enabled 应为 false");
        assert_eq!(n.source, "local", "手工安装未带 .market 标记来源应为 local（禁用目录也能识别）");
        let tools = mgr.list_tools(None).unwrap();
        assert!(!tools.iter().any(|s| s.name == "nodejs"), "禁用的 工具 不应出现在 list_tools");

        // 3. 重新启用
        mgr.toggle_plugin("nodejs", true).unwrap();
        assert!(paths.plugins.join("nodejs").join("plugin.lua").exists());
        let infos = mgr.list_plugins();
        let n = infos.iter().find(|p| p.name == "nodejs").unwrap();
        assert!(n.enabled, "重新启用后 enabled 应为 true");
        let tools = mgr.list_tools(None).unwrap();
        assert!(tools.iter().any(|s| s.name == "nodejs"), "重新启用后应回到 list_tools");

        // 4. 禁用后重建 manager 不应重新注入
        mgr.toggle_plugin("nodejs", false).unwrap();
        let mgr2 = EnvHiveManager::new(AppConfig::default(), paths.clone(), reqwest::Client::new());
        let infos = mgr2.list_plugins();
        let n = infos.iter().find(|p| p.name == "nodejs").expect("禁用插件应仍显示");
        assert!(!n.enabled, "重建 manager 后禁用状态应保持");

        // 5. 重复禁用报错（幂等保护）
        assert!(mgr2.toggle_plugin("nodejs", false).is_err(), "重复禁用应报错");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 插件来源识别：本地（无 .market）→ local；带 .market 标记 → market（内置态已移除）
    #[test]
    fn test_plugin_source_classification() {
        let root = std::env::temp_dir().join(format!("envhive-mgr-src-{}", std::process::id()));
        let paths = pathmeta::from_root(&root);
        std::fs::create_dir_all(&paths.plugins).unwrap();
        let mgr = EnvHiveManager::new(AppConfig::default(), paths.clone(), reqwest::Client::new());

        // 初始：无任何插件
        assert!(mgr.list_plugins().is_empty(), "无内置插件，列表应为空");

        // 本地 Lua 插件 → local
        envhive_toolkit::lua_plugin::add_lua_plugin(&paths, "demo", "TOOL = { name='demo', display='Demo', category='language', homepage='https://x', verify_bin='demo', verify_arg='--version', bin_suffix='' }\nfunction available() return { '1.0.0' } end\nfunction pre_install(ctx) return { url = 'https://x/demo-' .. ctx.version } end\n").unwrap();
        let infos = mgr.list_plugins();
        let d = infos.iter().find(|p| p.name == "demo").unwrap();
        assert_eq!(d.source, "local", "未带 .market 标记应为 local");
        assert_eq!(d.provider, "lua");
        assert!(d.updated_at.is_some(), "updated_at 应取 plugin.lua mtime");
        assert!(!d.path.is_empty(), "path 应返回插件目录");

        // 打 .market 标记 → market
        std::fs::write(paths.plugins.join("demo").join(".market"), "remote").unwrap();
        let infos = mgr.list_plugins();
        let d = infos.iter().find(|p| p.name == "demo").unwrap();
        assert_eq!(d.source, "market", "带 .market 标记应为 market");
        let _ = std::fs::remove_dir_all(&root);
    }
}
