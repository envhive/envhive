//! 内置受限模块注入：http / json / archiver / file / versions

use std::path::{Path, PathBuf};

use mlua::{Lua, LuaSerdeExt, Table, Value};

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};

/// mlua 错误 → EnvHiveError（模块注入上下文）
fn wrap_lua(e: mlua::Error, ctx: &str) -> EnvHiveError {
    EnvHiveError::with_source(EnvHiveErrorKind::Internal, format!("{ctx}失败"), e)
}

/// 向 VM 全局注入受限模块
/// `pub(super)`：被 vm::new_plugin_lua 调用。
pub(super) fn inject_modules(lua: &Lua, data_root: &Path) -> Result<()> {
    inject_http(lua).map_err(|e| wrap_lua(e, "注入 http 模块"))?;
    inject_json(lua).map_err(|e| wrap_lua(e, "注入 json 模块"))?;
    inject_archiver(lua).map_err(|e| wrap_lua(e, "注入 archiver 模块"))?;
    inject_file(lua, data_root).map_err(|e| wrap_lua(e, "注入 file 模块"))?;
    inject_versions(lua).map_err(|e| wrap_lua(e, "注入 versions 模块"))?;
    Ok(())
}

/// 插件内网络代理（应用内生效）：由 Manager 在代理配置变化时同步更新。
/// http.get 读取该值构建带代理的 blocking client，保证插件请求与宿主下载同一代理链路。
static PLUGIN_PROXY: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

/// 更新插件内 http.get 的代理（None = 不使用代理）
pub fn set_plugin_proxy(url: Option<&str>) {
    *PLUGIN_PROXY.write().unwrap_or_else(|e| e.into_inner()) = url.map(String::from);
}

/// blocking HTTP client（独立连接池，不依赖异步 runtime）
fn blocking_client() -> &'static reqwest::blocking::Client {
    static CLIENT: std::sync::OnceLock<reqwest::blocking::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .user_agent("envhive/0.1.0 (lua-plugin)")
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_default()
    })
}

/// 按当前插件代理配置取 HTTP client：启用代理时每次构建（代理地址动态可换），
/// 未启用时复用共享的无代理 client。插件请求频率极低，构建开销可忽略。
fn plugin_http_client() -> reqwest::blocking::Client {
    let proxy_url = PLUGIN_PROXY.read().unwrap_or_else(|e| e.into_inner()).clone();
    match proxy_url {
        Some(url) => {
            let mut builder = reqwest::blocking::Client::builder()
                .user_agent("envhive/0.1.0 (lua-plugin)")
                .timeout(std::time::Duration::from_secs(60));
            if let Ok(p) = reqwest::Proxy::all(&url) {
                builder = builder.proxy(p);
            }
            builder.build().unwrap_or_default()
        }
        None => blocking_client().clone(),
    }
}

/// http.get(url) → body 字符串；http.head(url) → HTTP 状态码（轻量探测资源存在性）。
/// 网络白名单：插件声明 NETWORK_ALLOW 时仅允许列表内 host
/// （镜像替换后 host 亦放行——规则来自插件自身 mirrors 声明或用户配置，视为已授权）。
/// 防崩兜底：reqwest::blocking 在 tokio async 线程内使用会 panic（Cannot start a runtime from
/// within a runtime），此处用 catch_unwind 包裹，误用时降级为 Lua 错误而不是拖垮进程。
fn inject_http(lua: &Lua) -> mlua::Result<()> {
    let http = lua.create_table()?;

    // 前置处理：URL 协议校验 + 镜像前缀替换 + 网络白名单检查，返回实际请求 URL
    fn prepare_http_url(lua: &Lua, url: &str) -> mlua::Result<String> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(mlua::Error::external("http 仅支持 http/https URL"));
        }
        // 下载加速镜像：先按 __envhive_mirror_rules（from→to）做前缀替换，
        // 保证插件内请求（版本列表 / checksum / 探测）与宿主下载走同一镜像。
        let mut effective = url.to_string();
        if let Ok(rules) = lua.globals().get::<Table>("__envhive_mirror_rules") {
            for pair in rules.pairs::<String, String>() {
                if let Ok((from, to)) = pair {
                    if effective.starts_with(&from) {
                        effective = effective.replacen(&from, &to, 1);
                        break;
                    }
                }
            }
        }
        // 白名单检查（插件可声明 NETWORK_ALLOW = { "host", ... }；镜像目标 host 视为已授权）
        let allow: Option<Vec<String>> = lua.globals().get("NETWORK_ALLOW").ok();
        if let Some(list) = allow {
            let mut allowed = list;
            if let Ok(rules) = lua.globals().get::<Table>("__envhive_mirror_rules") {
                for pair in rules.pairs::<String, String>() {
                    if let Ok((_, to)) = pair {
                        if let Some(h) = host_of(&to) {
                            allowed.push(h);
                        }
                    }
                }
            }
            let host = host_of(&effective).unwrap_or_default();
            if !allowed.iter().any(|h| h == &host) {
                return Err(mlua::Error::external(format!(
                    "http 请求被网络白名单拦截：{host}（请在 NETWORK_ALLOW 中声明）"
                )));
            }
        }
        Ok(effective)
    }

    let get = lua.create_function(|lua, url: String| {
        let effective = prepare_http_url(lua, &url)?;
        // blocking send 可能因当前线程处于 tokio runtime 而 panic —— 捕获并转错误
        let client = plugin_http_client();
        let send = || client.get(&effective).send();
        let resp = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(send)) {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => return Err(mlua::Error::external(format!("http.get 请求失败: {e}"))),
            Err(_) => {
                return Err(mlua::Error::external(
                    "http.get 不能在异步线程中使用（请由 available/pre_install 等网络 hook 调用）",
                ))
            }
        };
        let status = resp.status();
        if !status.is_success() {
            return Err(mlua::Error::external(format!("http.get HTTP {status}: {effective}")));
        }
        resp.text().map_err(|e| mlua::Error::external(format!("http.get 读取响应失败: {e}")))
    })?;

    // http.head(url) → 状态码（200/404/…）：供插件探测资源存在性（如 dlcdn 是否保留旧版本）
    let head = lua.create_function(|lua, url: String| {
        let effective = prepare_http_url(lua, &url)?;
        let client = plugin_http_client();
        let send = || client.head(&effective).send();
        let resp = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(send)) {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => return Err(mlua::Error::external(format!("http.head 请求失败: {e}"))),
            Err(_) => {
                return Err(mlua::Error::external(
                    "http.head 不能在异步线程中使用（请由 available/pre_install 等网络 hook 调用）",
                ))
            }
        };
        Ok(resp.status().as_u16())
    })?;
    http.set("get", get)?;
    http.set("head", head)?;
    lua.globals().set("http", http)
}

/// 提取 URL 的 host（含端口剥离）
fn host_of(url: &str) -> Option<String> {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .map(|h| h.split(':').next().unwrap_or(h).to_string())
}

/// json.encode(obj) / json.decode(str)
fn inject_json(lua: &Lua) -> mlua::Result<()> {
    let json = lua.create_table()?;
    let encode = lua.create_function(|lua, v: Value| {
        let jv: serde_json::Value = lua.from_value(v)?;
        serde_json::to_string(&jv).map_err(mlua::Error::external)
    })?;
    let decode = lua.create_function(|lua, s: String| {
        let jv: serde_json::Value = serde_json::from_str(&s).map_err(mlua::Error::external)?;
        lua.to_value(&jv)
    })?;
    json.set("encode", encode)?;
    json.set("decode", decode)?;
    lua.globals().set("json", json)
}

/// archiver.extract(archive, dest) — 复用安装管道解压器
fn inject_archiver(lua: &Lua) -> mlua::Result<()> {
    let archiver = lua.create_table()?;
    let extract = lua.create_function(|_, (archive, dest): (String, String)| {
        let kind = envhive_core::util::CompressKind::from_url(&archive);
        crate::tool::install::decompress_archive(std::path::Path::new(&archive), std::path::Path::new(&dest), kind)
            .map_err(|e| mlua::Error::external(format!("archiver.extract 失败: {e}")))?;
        Ok(true)
    })?;
    archiver.set("extract", extract)?;
    lua.globals().set("archiver", archiver)
}

/// file.read/write/exists — 路径限制：仅允许 ~/.envhive 与系统临时目录内、无 `..`
fn inject_file(lua: &Lua, data_root: &Path) -> mlua::Result<()> {
    let root = data_root.canonicalize().unwrap_or_else(|_| data_root.to_path_buf());
    let temp = std::env::temp_dir();
    let file = lua.create_table()?;

    let root_a = root.clone();
    let temp_a = temp.clone();
    let read = lua.create_function(move |_, p: String| {
        let path = check_lua_path(&root_a, &temp_a, &p)?;
        std::fs::read_to_string(&path).map_err(|e| mlua::Error::external(format!("file.read 失败: {e}")))
    })?;
    let root_b = root.clone();
    let temp_b = temp.clone();
    let write = lua.create_function(move |_, (p, content): (String, String)| {
        let path = check_lua_path(&root_b, &temp_b, &p)?;
        std::fs::write(&path, content).map_err(|e| mlua::Error::external(format!("file.write 失败: {e}")))?;
        Ok(true)
    })?;
    let root_c = root.clone();
    let temp_c = temp.clone();
    let exists = lua.create_function(move |_, p: String| {
        let path = check_lua_path(&root_c, &temp_c, &p)?;
        Ok(path.exists())
    })?;
    file.set("read", read)?;
    file.set("write", write)?;
    file.set("exists", exists)?;
    lua.globals().set("file", file)
}

/// file 模块路径校验：无 `..`、且位于 root 或系统临时目录内
fn check_lua_path(root: &std::path::Path, temp: &std::path::Path, p: &str) -> mlua::Result<PathBuf> {
    let path = PathBuf::from(p);
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(mlua::Error::external("file 模块禁止 '..' 路径"));
    }
    let canon = path.canonicalize().unwrap_or(path.clone());
    if !canon.starts_with(root) && !canon.starts_with(temp) {
        return Err(mlua::Error::external(format!(
            "file 模块仅允许访问 {} 与系统临时目录",
            root.display()
        )));
    }
    Ok(canon)
}

/// versions.parse(id) → SDKMAN 风格版本标识符解析（内置模块，参照 vfox distribution_version.lua）
/// 输入：`<version>[.fx]-<dist短名>`（如 "26.0.2-zulu" / "26.0.2.fx-zulu"）
/// 输出：{ version="26.0.2", distribution="zulu", javafx=true }
/// 规则：按最后一个 '-' 拆出发行商；版本段以 ".fx" 结尾 → javafx=true 并剥去。
/// 无 '-'（纯版本号）→ distribution=nil，仅解析 .fx 标记。
fn inject_versions(lua: &Lua) -> mlua::Result<()> {
    let versions = lua.create_table()?;
    let parse = lua.create_function(|lua, s: String| {
        let out = lua.create_table()?;
        if s.is_empty() {
            return Ok(out);
        }
        // 最后一个 '-'：左段 = 版本+fx 段，右段 = 发行商短名
        let (rest, dist) = match s.rfind('-') {
            Some(idx) if idx + 1 < s.len() => (&s[..idx], Some(&s[idx + 1..])),
            _ => (s.as_str(), None),
        };
        // ".fx" 标记（SDKMAN 风格：26.0.2.fx-zulu）
        let (version, javafx) = match rest.strip_suffix(".fx") {
            Some(v) if !v.is_empty() => (v.to_string(), true),
            _ => (rest.to_string(), false),
        };
        out.set("version", version)?;
        if let Some(d) = dist {
            if !d.is_empty() {
                out.set("distribution", d)?;
            }
        }
        out.set("javafx", javafx)?;
        Ok(out)
    })?;
    versions.set("parse", parse)?;
    lua.globals().set("versions", versions)
}
