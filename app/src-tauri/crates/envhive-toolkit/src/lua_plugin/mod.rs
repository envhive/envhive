//! Lua 插件支持（roadmap P2：完整脚本能力，对应 vfox gopher-lua 插件；多发行商协议）
//!
//! 插件文件：`~/.envhive/plugins/<name>/plugin.lua`
//! ```lua
//! TOOL = {
//!   name = "python", display = "Python", category = "language",
//!   homepage = "https://www.python.org",
//!   verify_bin = "python", verify_arg = "--version", bin_suffix = "",
//!   root_hint = nil,
//!   env_vars = { PYTHON_HOME = "{root}" },   -- 可选，{root} 为版本链接目录
//!
//!   -- 发行商维度（可选）。声明则前端渲染「发行商」下拉 →「版本」下拉两级联动；
//!   -- 不声明（如 nodejs）则无此维度。
//!   distributions = {
//!     { key = "open", display = "OpenJDK (Eclipse)", default = true },
//!     { key = "tem",  display = "Eclipse Temurin" },
//!   },
//!   default_distribution = "open",   -- 可选；缺省取 distributions[1]
//! }
//! NETWORK_ALLOW = { "www.python.org", "mirrors.aliyun.com" }  -- 可选网络白名单
//!
//! function available(ctx)          -- ctx.distribution = 选中发行商 key；返回版本数组（不重排，信任插件顺序）
//! function pre_install(ctx)        -- ctx={version,distribution,os,arch} → {url=..., file_name=..., checksum=...} 或 nil
//! function post_install(ctx)       -- 可选：解压完成后调用；ctx={root,version,distribution}
//! function env_keys(ctx)           -- ctx={root} → {vars={...}, paths={...}} 或 nil
//! function pre_uninstall(ctx)      -- 可选清理钩子
//! ```
//!
//! 安全模型（可信插件模型）：
//! - **放开全部标准库**（含 os / io / debug 之外的全部；可 os.execute / io.popen / print，对齐 vfox）。
//!   仅导入可信来源的插件 —— UI 添加插件处有提示。
//! - 内置受限模块：`http.get(url)`（网络白名单 NETWORK_ALLOW）、`json.encode/decode`、
//!   `archiver.extract(file, dir)`、`file.read/write/exists`（仅 ~/.envhive 与系统临时目录）、
//!   `versions.parse(id)`（SDKMAN 风格 `<version>[.fx]-<dist>` 标识符解析）。
//! - `lib/` 子目录（可选）：`~/.envhive/plugins/<name>/lib/*.lua` 注册为可 require 模块
//!   （对齐 vfox 的 lib 约定）。
//!
//! 崩溃防护（2026-08 修复）：
//! 1. mlua 启用 `send` feature 后 Lua VM 跨线程使用/销毁是 UB —— 本模块改为
//!    **每次 hook 调用在调用线程内新建 VM → 执行 → 销毁**，同一线程内完成，
//!    不再共享 `Arc<Mutex<Lua>>`，消除闪退根因。
//! 2. hook 内的 `http.get` 走 blocking HTTP，在 tokio async 线程内调用会 panic ——
//!    网络型 hook（available / pre_install）由调用方放入 `spawn_blocking` 执行，
//!    且 `http.get` 内部用 `catch_unwind` 兜底，误用时降级为错误而非崩溃。
//! 3. 加载时强制校验必需 hook（available / pre_install）存在，坏插件导入即拒，
//!    不会等到运行时才炸。
//!
//! 模块划分：
//! - `def`：插件定义解析与文件管理（加载 / 校验 / 新增 / 更新 / 默认插件注入）
//! - `vm`：Lua VM 生命周期与 hooks（每次调用新建 VM → 执行 → 销毁）
//! - `modules`：内置受限模块注入（http / json / archiver / file / versions）
//! - `info`：前端展示信息转换

mod def;
mod info;
mod modules;
mod vm;

pub use def::*;
pub use info::*;
pub use modules::*;
pub use vm::*;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use envhive_core::env::Envs;
use envhive_core::error::{EnvHiveError, EnvHiveErrorKind};
use envhive_core::pathmeta::PathMeta;
use crate::tool::{DistributionInfo, ToolDescriptor};

/// mlua 错误 → EnvHiveError（统一错误模型）
/// 注：EnvHiveError 现定义于 envhive-core（外部 crate），孤儿规则禁止 `impl From<mlua::Error> for EnvHiveError`，
/// 改用显式转换函数，调用处 `map_err(lua_err)`。
pub(crate) fn lua_err(e: mlua::Error) -> EnvHiveError {
    EnvHiveError::with_source(EnvHiveErrorKind::Internal, format!("Lua 执行错误: {e}"), e)
}

// ---------------------------------------------------------------------------
// 插件定义
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct LuaPluginDef {
    pub name: String,
    pub display: String,
    pub category: String,
    pub homepage: String,
    /// 插件自身版本号（TOOL.version 可选声明；市场 manifest 有同名插件时可据此判断更新）
    pub version: Option<String>,
    pub verify_bin: String,
    pub verify_arg: String,
    pub bin_suffix: String,
    /// 静态 root_hint（工具 表声明）
    pub root_hint: Option<String>,
    /// 环境变量（值支持 {root} 占位符）
    pub env_vars: Vec<(String, String)>,
    /// 发行商维度（TOOL.distributions 声明；空 = 无发行商维度，如 nodejs）
    pub distributions: Vec<DistributionInfo>,
    /// 缺省发行商 key（TOOL.default_distribution；缺省取 distributions[0]）
    pub default_distribution: Option<String>,
    /// 下载加速镜像候选（TOOL.mirrors 声明；每项 { name, from, to }，前端可切换多地址）
    pub mirrors: Vec<crate::tool::MirrorCandidate>,
    /// 缺省加速镜像名（TOOL.default_mirror；缺省取 mirrors 首个非官方候选）
    pub default_mirror: Option<String>,
    /// 工具 图标声明（TOOL.icon_base64：base64 原文或 data URI；图标文件 icon.svg/png/jpg 优先于声明）
    pub icon_base64: Option<String>,
    /// 可选私有模块目录 `plugins/<name>/lib/`（该目录下 .lua 可被 require）
    pub lib_dir: PathBuf,
    /// 插件脚本原文（每次 hook 调用重建 VM 执行）
    script: String,
    /// pre_install 可动态指定的 root_hint（跨 hook 传递，Rust 侧缓存）
    dynamic_root_hint: Arc<Mutex<Option<String>>>,
    /// 数据根目录（file 模块路径限制用）
    data_root: PathBuf,
}

impl std::fmt::Debug for LuaPluginDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LuaPluginDef({})", self.name)
    }
}

impl ToolDescriptor for LuaPluginDef {
    fn name(&self) -> &str { &self.name }
    fn display(&self) -> &str { if self.display.is_empty() { &self.name } else { &self.display } }
    fn category(&self) -> &str { &self.category }
    fn homepage(&self) -> &str { &self.homepage }
    fn provider(&self) -> crate::tool::provider::ProviderKind { crate::tool::provider::ProviderKind::Lua }
    fn gh_repo(&self) -> Option<(&str, &str)> { None }
    fn static_index_url(&self) -> Option<&str> { None }
    fn url_template(&self) -> Option<&str> { None }
    fn platform(&self) -> Option<&crate::tool::PlatformMap> { None }
    fn verify_bin(&self) -> &str { &self.verify_bin }
    fn verify_arg(&self) -> &str { &self.verify_arg }
    fn bin_suffix(&self) -> &str { &self.bin_suffix }
    fn env_vars(&self) -> &[(&str, crate::tool::EnvVarKind)] { &[] }
    fn root_hint(&self) -> Option<&str> { self.root_hint.as_deref() }
    fn distributions(&self) -> &[DistributionInfo] { &self.distributions }
    fn default_distribution(&self) -> Option<&str> { self.default_distribution.as_deref() }
    fn mirrors(&self) -> &[crate::tool::MirrorCandidate] { &self.mirrors }
    fn default_mirror(&self) -> Option<&str> { self.default_mirror.as_deref() }
    fn icon_base64(&self) -> Option<&str> { self.icon_base64.as_deref() }
    fn lua_def(&self) -> Option<&LuaPluginDef> { Some(self) }
}

/// Lua 插件 工具 实例
pub struct LuaTool {
    pub plugin: Arc<LuaPluginDef>,
}

impl crate::tool::Tool for LuaTool {
    fn desc(&self) -> &dyn ToolDescriptor {
        &*self.plugin
    }

    /// 插件环境变量注入（env_keys hook + 工具.env_vars + 默认 PATH）
    fn extra_env_keys(&self, paths: &PathMeta, scope_active: bool) -> Envs {
        if !scope_active {
            return Envs::new();
        }
        let root = paths.current_link(&self.plugin.name);
        let root_str = root.to_string_lossy().to_string();
        let mut envs = hook_env_keys(&self.plugin, &root_str);
        // 默认 bin PATH（prepend_path 按路径语义等价去重：
        // Lua hook 返回的 `root .. "/bin"` 与 PathBuf 的 `root\bin` 视为同一条）
        let bin = root.join(self.plugin.bin_suffix.trim_start_matches('/'));
        envs.prepend_path(bin.to_string_lossy().to_string());
        // 工具.env_vars（{root} 占位符）
        for (k, v) in &self.plugin.env_vars {
            envs.var(k.clone(), v.replace("{root}", &root_str));
        }
        envs
    }
}

#[cfg(test)]
mod tests {
    use super::def::build_plugin_def;
    use super::*;
    use envhive_core::pathmeta;

    fn tmp_paths(tag: &str) -> (PathMeta, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("envhive-lua-{tag}-{}", std::process::id()));
        let paths = pathmeta::from_root(&root);
        std::fs::create_dir_all(&paths.plugins).unwrap();
        (paths, root)
    }

    /// 读取仓库插件源码 fixture：`<repo>/plugins/src/<name>/plugin.lua`
    /// （插件不再内置，源码统一维护在仓库 plugins/src/ 下）
    fn fixture_script(name: &str) -> String {
        // CARGO_MANIFEST_DIR = <repo>/app/src-tauri/crates/envhive-toolkit → 上溯 4 级到仓库根
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../plugins/src");
        std::fs::read_to_string(base.join(name).join("plugin.lua"))
            .unwrap_or_else(|e| panic!("fixture 插件 {name} 缺失: {e}"))
    }

    const SCRIPT: &str = r#"
TOOL = {
  name = "demo",
  display = "Demo",
  category = "language",
  homepage = "https://example.com",
  verify_bin = "demo",
  verify_arg = "--version",
  bin_suffix = "",
}

NETWORK_ALLOW = { "example.com" }

function available()
  return { "1.0.0", "1.1.0", "2.0.0-rc1" }
end

function pre_install(ctx)
  return {
    url = "https://example.com/demo-" .. ctx.version .. "-" .. ctx.os .. "-" .. ctx.arch .. ".zip",
    checksum = "sha256:abc123",
  }
end

function env_keys(ctx)
  return { vars = { DEMO_HOME = ctx.root }, paths = { ctx.root .. "/bin" } }
end
"#;

    #[test]
    fn test_load_and_hooks() {
        let (paths, root) = tmp_paths("load");
        add_lua_plugin(&paths, "demo", SCRIPT).unwrap();
        let def = load_lua_plugin(&paths, "demo").unwrap().expect("插件应加载成功");

        assert_eq!(def.name, "demo");
        assert_eq!(def.display, "Demo");
        assert_eq!(def.verify_bin, "demo");
        // SCRIPT 未声明 version → None（不影响解析）
        assert_eq!(def.version, None);

        let versions = hook_available(&def, None, &[]).unwrap();
        let vs: Vec<String> = versions.iter().map(|v| v.version.clone()).collect();
        assert!(vs.contains(&"1.0.0".to_string()));
        assert!(vs.contains(&"2.0.0-rc1".to_string()));

        let pkg = resolve_lua_package(&def, "1.1.0", None, &[]).unwrap();
        assert!(pkg.url.contains("demo-1.1.0"));
        assert_eq!(pkg.checksum.as_deref(), Some("sha256:abc123"));

        let envs = hook_env_keys(&def, "/fake/root");
        assert_eq!(envs.vars.get("DEMO_HOME").and_then(|v| v.clone()).as_deref(), Some("/fake/root"));
        assert!(envs.paths.contains(&"/fake/root/bin".to_string()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_invalid_script_rejected() {
        let (paths, root) = tmp_paths("bad");
        assert!(add_lua_plugin(&paths, "bad", "function available( return {").is_err());
        assert!(add_lua_plugin(&paths, "bad2", "x = 1").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// TOOL.version 可选声明：解析 → LuaPluginDef → PluginInfo 透传（市场更新判断依据）
    #[test]
    fn test_plugin_version_field() {
        let (paths, root) = tmp_paths("ver");
        let script = r#"
TOOL = {
  name = "demo",
  display = "Demo",
  category = "language",
  homepage = "https://example.com",
  version = "2.1.0",
  verify_bin = "demo",
  verify_arg = "--version",
  bin_suffix = "",
}
function available() return { "1.0.0" } end
function pre_install(ctx) return { url = "https://example.com/demo-" .. ctx.version } end
"#;
        add_lua_plugin(&paths, "demo", script).unwrap();
        let def = load_lua_plugin(&paths, "demo").unwrap().expect("插件应加载成功");
        assert_eq!(def.version.as_deref(), Some("2.1.0"));
        let info = to_plugin_info(&paths, &def);
        assert_eq!(info.version.as_deref(), Some("2.1.0"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 缺少必需 hook（available / pre_install）的脚本必须在导入时被拒绝
    #[test]
    fn test_missing_hook_rejected() {
        let (paths, root) = tmp_paths("nohook");
        let script = r#"
TOOL = { name = "x", verify_bin = "x" }
function pre_install(ctx) return { url = "https://example.com/x" } end
"#;
        let err = add_lua_plugin(&paths, "x", script).unwrap_err();
        assert!(err.to_string().contains("available()"), "应提示缺少 available()，实际: {err}");
        // 文件不应残留
        assert!(!plugin_file(&paths, "x").exists());

        let script2 = r#"
TOOL = { name = "y", verify_bin = "y" }
function available() return {} end
"#;
        let err2 = add_lua_plugin(&paths, "y", script2).unwrap_err();
        assert!(err2.to_string().contains("pre_install()"), "应提示缺少 pre_install()，实际: {err2}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 可信沙箱：标准库放开，os / io / package 均可用（对齐 vfox），脚本可执行命令。
    /// 恶意脚本无法拖垮进程仍由 hook 调用方（spawn_blocking + catch_unwind）兜底。
    #[test]
    fn test_trusted_lib_enabled() {
        let (paths, root) = tmp_paths("trusted");
        // os.time / io.open / package 均可用 → 加载成功
        let script = r#"
TOOL = { name = "trusted", verify_bin = "trusted" }
local t = os.time()
local ok_pkg = type(package) == "table"
function available() return {} end
function pre_install(ctx) return { url = "https://example.com/e" } end
"#;
        assert!(add_lua_plugin(&paths, "trusted", script).is_ok(), "可信模型下 os/package 应可用");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// TOOL.distributions / default_distribution 解析，前端可渲染「发行商」下拉
    #[test]
    fn test_distributions_parsed() {
        let (paths, root) = tmp_paths("dist");
        let script = r#"
TOOL = {
  name = "java", verify_bin = "bin/java",
  distributions = {
    { key = "open", display = "OpenJDK (Eclipse)", default = true },
    { key = "tem",  display = "Eclipse Temurin" },
    { key = "zulu", display = "Azul Zulu" },
  },
}
function available(ctx) return { { version = "26.0.2-" .. (ctx.distribution or "open"), labels = { "stable" } } } end
function pre_install(ctx) return { url = "https://example.com/" .. ctx.version .. "-" .. (ctx.distribution or "x") } end
"#;
        add_lua_plugin(&paths, "java", script).unwrap();
        let def = load_lua_plugin(&paths, "java").unwrap().expect("插件应加载成功");

        assert_eq!(def.distributions.len(), 3);
        assert_eq!(def.distributions[0].key, "open");
        assert_eq!(def.distributions[1].display, "Eclipse Temurin");
        // 未声明 default_distribution → 取 default=true 项
        assert_eq!(def.default_distribution.as_deref(), Some("open"));

        // available(ctx) 注入 distribution：显式传入 tem
        let versions = hook_available(&def, Some("tem"), &[]).unwrap();
        assert_eq!(versions[0].version, "26.0.2-tem");
        // 无选择时调用方已解析 default → 显式传 open 与 default 一致
        let versions2 = hook_available(&def, Some("open"), &[]).unwrap();
        assert_eq!(versions2[0].version, "26.0.2-open");

        // pre_install ctx.distribution 注入
        let pkg = resolve_lua_package(&def, "26.0.2-tem", Some("tem"), &[]).unwrap();
        assert!(pkg.url.ends_with("-tem"), "URL 应包含发行商，实际: {}", pkg.url);

        // 无发行商维度的插件：ctx.distribution 为 nil（兼容无发行商插件）
        let (paths2, root2) = tmp_paths("dist2");
        let node_script = r#"
TOOL = { name = "nodejs", verify_bin = "node" }
function available() return { { version = "22.14.0", labels = { "lts" } } } end
function pre_install(ctx) return { url = "https://example.com/n-" .. ctx.version } end
"#;
        add_lua_plugin(&paths2, "nodejs", node_script).unwrap();
        let ndef = load_lua_plugin(&paths2, "nodejs").unwrap().unwrap();
        assert!(ndef.distributions.is_empty());
        let nv = hook_available(&ndef, None, &[]).unwrap();
        assert_eq!(nv[0].version, "22.14.0");

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&root2);
    }

    /// 内置模块 versions.parse：SDKMAN 风格 `<version>[.fx]-<dist>` 解析
    #[test]
    fn test_versions_parse_module() {
        let (paths, root) = tmp_paths("vparse");
        let script = r#"
TOOL = { name = "jp", verify_bin = "bin/java" }
function available()
  local a = versions.parse("26.0.2-zulu")
  local b = versions.parse("26.0.2.fx-zulu")
  local c = versions.parse("17.0.13-tem")
  local d = versions.parse("1.1.0")
  assert(a.version == "26.0.2" and a.distribution == "zulu" and a.javafx == false)
  assert(b.version == "26.0.2" and b.distribution == "zulu" and b.javafx == true)
  assert(c.distribution == "tem")
  assert(d.version == "1.1.0" and d.distribution == nil and d.javafx == false)
  return {}
end
function pre_install(ctx) return { url = "https://example.com/x" } end
"#;
        add_lua_plugin(&paths, "jp", script).unwrap();
        // 触发 available() 内部的 versions.parse 断言
        let versions = hook_available(&load_lua_plugin(&paths, "jp").unwrap().unwrap(), None, &[]).unwrap();
        assert!(versions.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// TOOL.mirrors / default_mirror 解析（前端可渲染「加速镜像」下拉）
    #[test]
    fn test_mirrors_parsed() {
        let (paths, root) = tmp_paths("mirror");
        let script = r#"
TOOL = {
  name = "nodejs", verify_bin = "node",
  mirrors = {
    { name = "npmmirror", from = "https://nodejs.org/dist/", to = "https://npmmirror.com/mirrors/node/" },
    { name = "华为云",     from = "https://nodejs.org/dist/", to = "https://mirrors.huaweicloud.com/nodejs/" },
  },
  default_mirror = "npmmirror",
}
function available() return { { version = "22.0.0", labels = { "stable" } } } end
function pre_install(ctx) return { url = "https://nodejs.org/dist/n-" .. ctx.version } end
"#;
        add_lua_plugin(&paths, "nodejs", script).unwrap();
        let def = load_lua_plugin(&paths, "nodejs").unwrap().expect("插件应加载成功");
        assert_eq!(def.mirrors.len(), 2);
        assert_eq!(def.mirrors[0].from, "https://nodejs.org/dist/");
        assert_eq!(def.mirrors[0].to, "https://npmmirror.com/mirrors/node/");
        assert_eq!(def.mirrors[1].name, "华为云");
        assert_eq!(def.default_mirror.as_deref(), Some("npmmirror"));
        // ToolDescriptor 透传
        assert_eq!(def.mirrors(), &def.mirrors);
        assert_eq!(def.default_mirror(), Some("npmmirror"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 镜像规则注入 —— hook_available 携带 mirror_rules 时，插件内 `_G.__envhive_mirror_rules`
    /// 可见（http.get 据此做前缀替换，保证插件内请求与下载同源）。
    #[test]
    fn test_mirror_rules_injected_into_vm() {
        let (paths, root) = tmp_paths("mirrorinj");
        let script = r#"
TOOL = { name = "nd", verify_bin = "node" }
function available()
  local rules = _G.__envhive_mirror_rules
  local from = rules and rules["https://nodejs.org/dist/"] or "none"
  return { { version = from, labels = { "stable" } } }
end
function pre_install(ctx) return { url = "https://example.com/x" } end
"#;
        add_lua_plugin(&paths, "nd", script).unwrap();
        let def = load_lua_plugin(&paths, "nd").unwrap().unwrap();
        // 注入规则：nodejs 官方前缀 → npmmirror
        let rules = vec![
            ("https://nodejs.org/dist/".to_string(), "https://npmmirror.com/mirrors/node/".to_string()),
        ];
        let versions = hook_available(&def, None, &rules).unwrap();
        assert_eq!(versions[0].version, "https://npmmirror.com/mirrors/node/");
        // 无规则时保持 "none"
        let versions2 = hook_available(&def, None, &[]).unwrap();
        assert_eq!(versions2[0].version, "none");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 可选 hook：post_install(ctx) 在解压完成后调用（ctx.root/version/distribution）
    /// 注：每次 hook 调用都是全新 VM（防跨线程 UB），post_install 的效果通过写文件验证。
    #[test]
    fn test_post_install_hook() {
        let (paths, root) = tmp_paths("postinst");
        let script = r#"
TOOL = { name = "pj", verify_bin = "pj" }
function available() return {} end
function pre_install(ctx) return { url = "https://example.com/p" } end
function post_install(ctx)
  -- file 模块允许写 data_root（= 测试临时目录）内路径
  file.write(ctx.root .. "/pi.txt",
             ctx.root .. "|" .. ctx.version .. "|" .. tostring(ctx.distribution or "nil"))
end
"#;
        add_lua_plugin(&paths, "pj", script).unwrap();
        let def = load_lua_plugin(&paths, "pj").unwrap().unwrap();
        let root_str = root.to_string_lossy().to_string();
        hook_post_install(&def, &root_str, "26.0.2.fx-zulu", Some("zulu"));
        let written = std::fs::read_to_string(root.join("pi.txt")).expect("post_install 应写入 pi.txt");
        assert_eq!(written, format!("{root_str}|26.0.2.fx-zulu|zulu"));

        // 未声明 post_install 的插件不报错
        let (paths2, root2) = tmp_paths("postinst2");
        add_lua_plugin(&paths2, "no", SCRIPT).unwrap();
        let ndef = load_lua_plugin(&paths2, "no").unwrap().unwrap();
        hook_post_install(&ndef, "/fake/root", "1.0.0", None); // 不 panic
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&root2);
    }

    /// 仓库插件（plugins/src 下 8 个）必须能通过加载校验：
    /// 语法正确 + 工具 元信息完整 + 必需 hook（available / pre_install）齐全。
    /// 注：不调用 hook（会发起网络请求），仅离线验证加载与元信息。
    /// verify_bin 可空（空 = 解压即用型 工具 跳过运行验证，如 Tomcat）。
    #[test]
    fn test_repo_plugins_load() {
        for name in ["nodejs", "java", "go", "rust", "python", "maven", "tomcat", "lua"] {
            let (paths, root) = tmp_paths("repo-defaults");
            let script = fixture_script(name);
            let def = build_plugin_def(name, &script, &paths.root)
                .unwrap_or_else(|e| panic!("仓库插件 {name} 应能加载: {e}"));
            assert_eq!(def.name, *name, "插件名应匹配");
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    /// 真实网络链路验证（手动运行：cargo test --lib -- --ignored network_verify_repo_plugins --nocapture）
    /// 依次调用各仓库插件的 available() / pre_install()，验证官方 API 结构与 URL 构造。
    #[test]
    #[ignore]
    fn network_verify_repo_plugins() {
        for name in ["nodejs", "java", "go", "rust", "python", "maven", "tomcat", "lua"] {
            let (paths, root) = tmp_paths("net");
            let script = fixture_script(name);
            let def = build_plugin_def(name, &script, &paths.root).unwrap();
            let versions = hook_available(&def, None, &[]).unwrap();
            println!("[{name}] available {} 个版本，最新: {:?}",
                versions.len(), versions.last().map(|v| &v.version));
            let Some(v) = versions.last() else { continue };
            let pkg = resolve_lua_package(&def, &v.version, None, &[]).unwrap();
            println!("[{name}] {} -> {}", v.version, pkg.url);
            assert!(!pkg.url.is_empty());
            let envs = hook_env_keys(&def, "/fake/root");
            assert!(!envs.paths.is_empty(), "[{name}] env_keys 应返回 PATH 条目");
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    /// Lua 单文件 工具 网络链路专项验证（手动运行：
    /// cargo test --lib -- --ignored network_verify_lua_plugin --nocapture）
    /// 验证 available() 版本清单 → pre_install() 资产 URL / SHA256SUMS checksum 匹配
    /// （checksum 条目带 build/<os>/ 路径前缀，按 basename 匹配）。
    #[test]
    #[ignore]
    fn network_verify_lua_plugin() {
        let (paths, root) = tmp_paths("lua-net");
        let script = fixture_script("lua");
        let def = build_plugin_def("lua", &script, &paths.root).unwrap();

        let versions = hook_available(&def, None, &[]).unwrap();
        assert_eq!(versions.len(), 4, "lua 版本清单应为 4 个");
        for v in &versions {
            assert!(
                v.version.matches('.').count() == 2 && !v.version.contains('-'),
                "lua 版本号应为 x.y.z：{}",
                v.version
            );
        }
        println!("[lua] available: {:?}", versions.iter().map(|v| &v.version).collect::<Vec<_>>());

        for v in &versions {
            let pkg = resolve_lua_package(&def, &v.version, None, &[]).unwrap();
            println!("[lua] {} -> {} (checksum: {:?})", v.version, pkg.url,
                pkg.checksum.as_ref().map(|c| &c[..c.len().min(24)]));
            assert!(!pkg.url.is_empty(), "{} 应有下载 URL", v.version);
            assert!(
                !pkg.url.ends_with(".zip") && !pkg.url.contains(".tar"),
                "lua 应为单文件（非归档）：{}",
                pkg.url
            );
            // 单文件资产应有 checksum（SHA256SUMS 按平台分发）
            assert!(
                pkg.checksum.as_deref().is_some_and(|c| c.starts_with("sha256:")),
                "{} 应有 sha256 checksum",
                v.version
            );
            // Windows：exe 依赖同目录 dll → extra_files 附带
            if cfg!(windows) {
                assert_eq!(pkg.extra_files.len(), 1, "{} Windows 应附带 luaVV.dll", v.version);
                let ef = &pkg.extra_files[0];
                assert!(ef.file_name.ends_with(".dll"), "附加文件应为 dll: {}", ef.file_name);
                assert!(!ef.url.is_empty());
                println!("[lua] {} extra: {} (checksum: {:?})", v.version, ef.file_name,
                    ef.checksum.as_ref().map(|c| &c[..c.len().min(24)]));
            } else {
                assert!(pkg.extra_files.is_empty(), "非 Windows 不应有附加文件");
            }
        }

        let envs = hook_env_keys(&def, "/fake/root");
        assert_eq!(envs.paths, vec!["/fake/root".to_string()], "lua 应注入根目录 PATH");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 端到端安装验证（手动运行：cargo test --lib -- --ignored install_lua_binary_e2e --nocapture）
    /// 真实下载 lua54.exe（Windows 另下载 lua54.dll 附加文件）→ install_binary 全链路：
    /// 统一命名（<tool>[.exe]）→ 附加文件 → 非 Windows 权限修复 → 运行验证（lua.exe -v）→ env_keys。
    #[test]
    #[ignore]
    fn install_lua_binary_e2e() {
        let (paths, root) = tmp_paths("lua-install");
        let script = fixture_script("lua");
        let def = build_plugin_def("lua", &script, &root).unwrap();

        // 1. 真实下载 lua54.exe（对应 5.4.8）；Windows 还需同目录 lua54.dll
        let cache = paths.tool_cache_dir("lua");
        std::fs::create_dir_all(&cache).unwrap();
        let mut extra_paths: Vec<std::path::PathBuf> = Vec::new();
        let file = cache.join("lua54.exe");
        {
            // 注意：写入句柄必须在此作用域内关闭 —— Windows 上未声明 FILE_SHARE_DELETE 的
            // 打开句柄会跟随文件对象（rename 后仍在），导致后续 CreateProcess 打开 exe 失败
            // （ERROR_SHARING_VIOLATION）。真实下载链路 download_and_verify 完成后句柄已关闭。
            let mut resp = reqwest::blocking::Client::new()
                .get("https://github.com/dyne/luabinaries/releases/download/54f813a/lua54.exe")
                .header("User-Agent", "envhive-test")
                .send()
                .unwrap()
                .error_for_status()
                .unwrap();
            let mut out = std::fs::File::create(&file).unwrap();
            std::io::copy(&mut resp, &mut out).unwrap();
        }
        if cfg!(windows) {
            let dll = cache.join("lua54.dll");
            {
                let mut resp = reqwest::blocking::Client::new()
                    .get("https://github.com/dyne/luabinaries/releases/download/54f813a/lua54.dll")
                    .header("User-Agent", "envhive-test")
                    .send()
                    .unwrap()
                    .error_for_status()
                    .unwrap();
                let mut out = std::fs::File::create(&dll).unwrap();
                std::io::copy(&mut resp, &mut out).unwrap();
            }
            extra_paths.push(dll);
        }

        // 2. 单文件安装（内部含运行验证 lua.exe -v）
        let ctx = crate::tool::install::InstallCtx {
            paths: &paths,
            desc: &def,
            version: "5.4.8",
            distribution: None,
        };
        let dir = crate::tool::install::install_binary(&ctx, &file, &extra_paths).unwrap();

        // 3. 统一命名 + 附加文件断言
        let exe_name = if cfg!(windows) { "lua.exe" } else { "lua" };
        let exe = dir.join(exe_name);
        assert!(exe.exists(), "统一命名可执行文件应存在: {}", exe.display());
        if cfg!(windows) {
            assert!(dir.join("lua54.dll").exists(), "Windows 应附带 lua54.dll");
        }

        // 4. 直接运行验证
        let ver = crate::tool::install::verify_install(&def, &dir).unwrap();
        println!("[lua] 安装验证输出: {ver}");
        assert!(ver.contains("Lua 5.4"), "验证输出应含 Lua 5.4: {ver}");

        // 5. env_keys 注入安装根目录
        let envs = hook_env_keys(&def, dir.to_string_lossy().as_ref());
        assert_eq!(envs.paths, vec![dir.to_string_lossy().to_string()]);

        let _ = std::fs::remove_dir_all(&root);
    }
}
