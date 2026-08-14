//! 环境解析器（envhive-toolkit）
//!
//! 从 PathMeta + 配置链计算合并的 Envs（Global + Project 工具版本 + 全局 env）。
//! 与 envhive-manager 的 `EnvHiveManager::resolve_envs` 逻辑等价，但零 Tauri 依赖，
//! 供 CLI（`envhive-cli load`）与桌面端共用。
//!
//! 工具 查找：Lua 插件（与 manager::lookup_tool 一致）。

use std::collections::HashMap;
use std::sync::Arc;

use envhive_core::env::Envs;
use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use envhive_core::pathmeta::PathMeta;
use envhive_core::toml_chain::ConfigChain;

use crate::lua_plugin::{load_lua_plugin, LuaTool};
use crate::tool::version::Version;
use crate::tool::Tool;

/// 工具 查找器：按名称加载工具实例（带进程内缓存，与桌面端行为一致）
#[derive(Default)]
pub struct ToolLookup {
    cache: std::sync::Mutex<HashMap<String, Arc<dyn Tool>>>,
}

impl ToolLookup {
    pub fn new() -> Self {
        Self::default()
    }

    /// 预填充缓存（供 manager 桥接已打开的 工具 实例；CLI 无需调用）
    pub fn seed(&self, name: &str, tool: Arc<dyn Tool>) {
        self.cache.lock().unwrap().insert(name.to_ascii_lowercase(), tool);
    }

    /// 查找工具：Lua 插件；命中缓存直接返回
    pub fn lookup(&self, paths: &PathMeta, name: &str) -> Result<Arc<dyn Tool>> {
        let key = name.to_ascii_lowercase();
        if let Some(tool) = self.cache.lock().unwrap().get(&key) {
            return Ok(tool.clone());
        }
        // Lua 插件
        if let Some(def) = load_lua_plugin(paths, &key)? {
            let tool: Arc<dyn Tool> = Arc::new(LuaTool { plugin: Arc::new(def) });
            self.cache.lock().unwrap().insert(key, tool.clone());
            return Ok(tool);
        }
        Err(EnvHiveError::new(
            EnvHiveErrorKind::ToolNotFound,
            format!("工具 {name} 未注册（可在「插件」页添加 Lua 插件）"),
        ))
    }
}

/// 计算当前链上合并的 Envs（Global + Project 工具版本 + 全局 env）
///
/// 语义与 manager::resolve_envs 一致：
/// - 跳过未安装 / 坏链接（current 链接不可解析）的工具，不注入环境
/// - PATH 前置工具 bin 目录，JAVA_HOME 等变量指向 current 链接
/// - 全局 env（config.yaml [env]）最后并入（覆盖工具变量）
pub fn resolve_envs(
    paths: &PathMeta,
    chain: &ConfigChain,
    global_env: &HashMap<String, String>,
    lookup: &ToolLookup,
) -> Result<Envs> {
    let mut envs = Envs::new();
    for (name, version) in chain.active_tools() {
        let Ok(tool) = lookup.lookup(paths, &name) else { continue };
        let v = Version::new(version.clone());
        if !tool.is_installed(paths, &v) {
            continue;
        }
        // 链接须真实有效（junction/符号链接可解析），空目录等坏链接不注入环境
        let active = envhive_core::env::symlink::read_link(&tool.current_link(paths)).is_ok();
        envs.extend(&tool.env_keys(paths, active));
        envs.extend(&tool.extra_env_keys(paths, active));
    }
    // 全局 env（config.yaml [env]）并入
    for (k, v) in global_env {
        envs.var(k.clone(), v.clone());
    }
    Ok(envs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use envhive_core::pathmeta;

    #[test]
    fn resolve_empty_chain_returns_empty() {
        let root = std::env::temp_dir().join(format!("envhive-resolver-{}", std::process::id()));
        let paths = pathmeta::from_root(&root);
        let chain = ConfigChain {
            global_path: paths.global_toml.clone(),
            global: Default::default(),
            project_path: None,
            project: None,
            session: None,
        };
        let lookup = ToolLookup::new();
        let envs = resolve_envs(&paths, &chain, &HashMap::new(), &lookup).unwrap();
        assert!(envs.is_empty(), "空配置链不应产生任何环境变量");
    }
}
