//! Lua VM 生命周期与 hooks
//!
//! 每次 hook 调用**新建 VM → 执行 → 销毁**（同一线程内完成），
//! 规避 mlua `send` feature 下跨线程使用/销毁的 UB（2026-08 崩溃修复）。

use std::path::Path;

use mlua::{Function, Lua, LuaSerdeExt, StdLib, Table, Value};

use envhive_core::env::Envs;
use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::tool::runtime::{AvailableVersion, RuntimePackage};

use super::lua_err;
use super::LuaPluginDef;
use super::modules::inject_modules;

/// 允许注入的标准库（可信插件模型：放开全部，含 IO/OS/PACKAGE，对齐 vfox）。
/// `ALL_SAFE` = base + package + coroutine + table + io + os + string + math + utf8
/// （仅排除 debug / ffi，避免插件直接干预宿主调试设施）。
fn safe_stdlib() -> StdLib {
    StdLib::ALL_SAFE
}

/// 新建受限插件 VM（安全标准库 + 注入受限模块 + lib/ 模块目录）
/// `pub(super)`：被 def::build_plugin_def 复用（校验用 VM）。
pub(super) fn new_plugin_lua(data_root: &Path, lib_dir: Option<&Path>) -> Result<Lua> {
    let lua = Lua::new_with(safe_stdlib(), mlua::LuaOptions::default())
        .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Internal, "创建 Lua VM 失败", e))?;
    inject_modules(&lua, data_root)?;
    setup_lib_path(&lua, lib_dir)?;
    Ok(lua)
}

/// 把 `plugins/<name>/lib/?.lua` 前置到 package.path（插件私有模块可 require）
pub(super) fn setup_lib_path(lua: &Lua, lib_dir: Option<&Path>) -> Result<()> {
    let Some(dir) = lib_dir else { return Ok(()) };
    if !dir.is_dir() {
        return Ok(());
    }
    let package: Table = lua.globals().get("package").map_err(|e| {
        EnvHiveError::with_source(EnvHiveErrorKind::Internal, "Lua 缺少 package 库", e)
    })?;
    let existing: String = package.get("path").unwrap_or_default();
    let sep = if existing.is_empty() { "" } else { ";" };
    let pattern = format!("{}{}/?.lua", dir.display(), std::path::MAIN_SEPARATOR);
    let merged = format!("{pattern};{sep}{existing}");
    package.set("path", merged).map_err(lua_err)?;
    Ok(())
}

/// 在全新 VM 中执行插件脚本，然后调用闭包 f。
/// VM 生命周期完全限定在本次调用内 —— 无跨线程共享、无跨线程 drop。
/// `mirror_rules`：下载加速镜像前缀替换规则（from→to），注入为 `__envhive_mirror_rules`，
/// 供 `http.get` 使用（保证插件内网络请求与宿主下载走同一镜像，checksum 同源）。
pub(super) fn with_vm<F, R>(def: &LuaPluginDef, mirror_rules: &[(String, String)], f: F) -> Result<R>
where
    F: FnOnce(&Lua, &Table) -> Result<R>,
{
    let lua = new_plugin_lua(&def.data_root, Some(&def.lib_dir))?;
    lua.load(&def.script).exec().map_err(|e| {
        tracing::error!("[{}] 插件脚本执行失败: {e}", def.name);
        EnvHiveError::with_source(
            EnvHiveErrorKind::Config,
            format!("执行 Lua 插件 {} 脚本失败", def.name),
            e,
        )
    })?;
    let globals = lua.globals();
    // 注入镜像替换规则表（http.get 闭包读取；无规则时跳过）
    if !mirror_rules.is_empty() {
        if let Ok(t) = lua.create_table() {
            for (from, to) in mirror_rules {
                let _ = t.set(from.as_str(), to.as_str());
            }
            let _ = globals.set("__envhive_mirror_rules", t);
        }
    }
    f(&lua, &globals)
}

// ---------------------------------------------------------------------------
// 生命周期 hooks（每次调用重建 VM）
// ---------------------------------------------------------------------------

/// available(ctx) → 版本列表
/// ctx.distribution = 选中发行商 key（无发行商维度插件为 nil）；顺序信任插件返回，不再重排。
/// `mirror_rules`：下载加速镜像前缀替换规则（from→to），注入 http.get，保证版本列表与下载同源。
pub fn hook_available(
    def: &LuaPluginDef,
    distribution: Option<&str>,
    mirror_rules: &[(String, String)],
) -> Result<Vec<AvailableVersion>> {
    with_vm(def, mirror_rules, |lua, globals| {
        let f: Function = globals.get("available").map_err(|_| {
            EnvHiveError::new(EnvHiveErrorKind::Config, format!("Lua 插件 {} 缺少 available() hook", def.name))
        })?;
        let ctx = lua.create_table().map_err(lua_err)?;
        if let Some(d) = distribution {
            ctx.set("distribution", d).map_err(lua_err)?;
        }
        let ret: Value = f.call(ctx).map_err(lua_err)?;
        let jv: serde_json::Value = lua.from_value(ret).map_err(lua_err)?;

        let mut out = Vec::new();
        // mlua 空表无法区分数组/对象，序列化为 {} —— 兼容空表 = 空版本列表
        let arr = match jv.as_array() {
            Some(a) => a,
            None => {
                if let Some(obj) = jv.as_object() {
                    if obj.is_empty() {
                        return Ok(Vec::new());
                    }
                }
                return Err(EnvHiveError::new(EnvHiveErrorKind::Config, "available() 应返回版本数组"));
            }
        };
        for item in arr {
            if let Some(v) = item.as_str() {
                if !v.is_empty() {
                    out.push(AvailableVersion { version: v.to_string(), lts: false, labels: vec!["stable".into()] });
                }
            } else if let Some(obj) = item.as_object() {
                let Some(ver) = obj.get("version").and_then(|v| v.as_str()) else { continue };
                let labels: Vec<String> = obj
                    .get("labels")
                    .and_then(|l| l.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_else(|| vec!["stable".into()]);
                out.push(AvailableVersion { version: ver.to_string(), lts: labels.contains(&"lts".to_string()), labels });
            }
        }
        Ok(out)
    })
    .map_err(|e| {
        tracing::error!("[{}] available() hook 失败: {e}", def.name);
        e
    })
}

/// pre_install(ctx) → 下载包（Lua 插件无 url_template 回退，缺失/返回 nil 均报错）
/// ctx 新增 distribution（选中发行商 key）
/// `mirror_rules`：下载加速镜像前缀替换规则（from→to），注入 http.get（如 Go 插件在
/// pre_install 内拉 .sha256，需与下载同源）。
pub fn resolve_lua_package(
    def: &LuaPluginDef,
    version: &str,
    distribution: Option<&str>,
    mirror_rules: &[(String, String)],
) -> Result<RuntimePackage> {
    let (os_alias, arch_alias) = crate::tool::provider::plugin_aliases(def);
    with_vm(def, mirror_rules, |lua, globals| {
        let f: Function = globals.get("pre_install").map_err(|_| {
            EnvHiveError::new(EnvHiveErrorKind::Config, format!("Lua 插件 {} 缺少 pre_install() hook", def.name))
        })?;
        let ctx = lua.create_table().map_err(lua_err)?;
        ctx.set("version", version).map_err(lua_err)?;
        if let Some(d) = distribution {
            ctx.set("distribution", d).map_err(lua_err)?;
        }
        ctx.set("os", os_alias).map_err(lua_err)?;
        ctx.set("arch", arch_alias).map_err(lua_err)?;
        ctx.set("ext", if cfg!(windows) { "zip" } else { "tar.gz" }).map_err(lua_err)?;

        let ret: Value = f.call(ctx).map_err(lua_err)?;
        if matches!(ret, Value::Nil) {
            return Err(EnvHiveError::new(
                EnvHiveErrorKind::Config,
                format!("Lua 插件 {} 的 pre_install({version}) 返回 nil（该版本可能无对应构建）", def.name),
            ));
        }
        let jv: serde_json::Value = lua.from_value(ret).map_err(lua_err)?;
        let url = jv
            .get("url")
            .and_then(|u| u.as_str())
            .ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Config, "pre_install() 应返回 {url=...}"))?
            .to_string();
        let file_name = jv
            .get("file_name")
            .and_then(|f| f.as_str())
            .map(String::from)
            .unwrap_or_else(|| url.rsplit('/').next().unwrap_or("download").to_string());
        let checksum = jv.get("checksum").and_then(|c| c.as_str()).map(String::from);
        // root_hint 可能由 pre_install 动态决定（存 Rust 侧，供安装阶段读取）
        if let Some(rh) = jv.get("root_hint").and_then(|r| r.as_str()) {
            *def.dynamic_root_hint.lock().unwrap_or_else(|e| e.into_inner()) = Some(rh.to_string());
        }
        // 附加文件（单文件 工具 的伴随资源，如 Windows dll）：宿主随主文件一并下载
        let mut extra_files = Vec::new();
        if let Some(arr) = jv.get("extra_files").and_then(|e| e.as_array()) {
            for item in arr {
                let Some(obj) = item.as_object() else { continue };
                let (Some(efu), Some(efn)) = (
                    obj.get("url").and_then(|u| u.as_str()),
                    obj.get("file_name").and_then(|f| f.as_str()),
                ) else {
                    continue;
                };
                extra_files.push(crate::tool::runtime::ExtraFile {
                    url: efu.to_string(),
                    file_name: efn.to_string(),
                    checksum: obj.get("checksum").and_then(|c| c.as_str()).map(String::from),
                });
            }
        }
        Ok(RuntimePackage {
            tool: def.name.clone(),
            version: version.to_string(),
            url,
            checksum,
            file_name,
            extra_files,
        })
    })
    .map_err(|e| {
        tracing::error!("[{}] pre_install({version}) hook 失败: {e}", def.name);
        e
    })
}

/// 动态 root_hint（pre_install 设置，安装时读取）
pub fn dynamic_root_hint(def: &LuaPluginDef) -> Option<String> {
    def.dynamic_root_hint.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// env_keys(ctx) → { vars, paths }（含 {root} 占位符替换）；hook 缺失返回空
pub fn hook_env_keys(def: &LuaPluginDef, root: &str) -> Envs {
    let mut envs = Envs::new();
    let result = with_vm(def, &[], |lua, globals| {
        let f: Option<Function> = globals.get("env_keys").ok();
        let Some(f) = f else { return Ok(()); };
        let ctx = match lua.create_table() {
            Ok(t) => t,
            Err(_) => return Ok(()),
        };
        if ctx.set("root", root).is_err() {
            return Ok(());
        }
        let ret: Value = match f.call(ctx) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("[{}] env_keys() hook 失败: {e}", def.name);
                return Ok(());
            }
        };
        if matches!(ret, Value::Nil) {
            return Ok(());
        }
        let Ok(jv) = lua.from_value::<serde_json::Value>(ret) else { return Ok(()) };
        if let Some(vars) = jv.get("vars").and_then(|v| v.as_object()) {
            for (k, v) in vars {
                let val = v.as_str().unwrap_or_default().replace("{root}", root);
                envs.var(k.clone(), val);
            }
        }
        if let Some(paths) = jv.get("paths").and_then(|p| p.as_array()) {
            for p in paths {
                if let Some(s) = p.as_str() {
                    // prepend_path 按路径语义等价去重（`\`/`/`、大小写差异视为同一条）
                    envs.prepend_path(s.replace("{root}", root));
                }
            }
        }
        Ok(())
    });
    if let Err(e) = result {
        tracing::warn!("[{}] env_keys() 执行异常，跳过插件环境注入: {e}", def.name);
    }
    envs
}

/// 卸载前钩子（尽力而为，失败仅告警）
pub fn hook_pre_uninstall(def: &LuaPluginDef, version: &str) {
    let result = with_vm(def, &[], |lua, globals| {
        let f: Option<Function> = globals.get("pre_uninstall").ok();
        let Some(f) = f else { return Ok(()); };
        let ctx = lua.create_table().map_err(lua_err)?;
        ctx.set("version", version).map_err(lua_err)?;
        f.call::<()>(ctx).map_err(lua_err)?;
        Ok(())
    });
    if let Err(e) = result {
        tracing::warn!("[{}] pre_uninstall hook 失败: {e}", def.name);
    }
}

/// 可选钩子 post_install(ctx)：解压完成、链接 current 之前调用。
/// ctx = { root, version, distribution }；典型用途：macOS 上把 jdk-<v>.jdk/Contents/Home/* 上移到 root。
/// 尽力而为：失败仅告警（安装本身不受影响）。
pub fn hook_post_install(def: &LuaPluginDef, root: &str, version: &str, distribution: Option<&str>) {
    let result = with_vm(def, &[], |lua, globals| {
        let f: Option<Function> = globals.get("post_install").ok();
        let Some(f) = f else { return Ok(()); };
        let ctx = lua.create_table().map_err(lua_err)?;
        ctx.set("root", root).map_err(lua_err)?;
        ctx.set("version", version).map_err(lua_err)?;
        if let Some(d) = distribution {
            ctx.set("distribution", d).map_err(lua_err)?;
        }
        f.call::<()>(ctx).map_err(lua_err)?;
        Ok(())
    });
    if let Err(e) = result {
        tracing::warn!("[{}] post_install hook 失败: {e}", def.name);
    }
}
