//! EnvHiveManager · 环境域：环境解析 / 进程注入 / 系统环境变量应用
//!
//! - `sync_user_env`：全局切换/卸载/应用时同步 Windows 用户注册表 PATH 与 工具 根变量
//! - `resolve_envs`：计算当前链上合并的 Envs（Global + Project 工具版本 + 全局 env）
//! - `spawn_with_env`：以此环境启动子进程
//! - `apply_global_env`：写入系统变量（UI v2）

use std::path::Path;

use envhive_core::env::Envs;
use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use envhive_core::toml_chain::ConfigChain;

use super::EnvHiveManager;

#[cfg(windows)]
use envhive_core::env::registry_windows;

impl EnvHiveManager {
    // -----------------------------------------------------------------------
    // 用户环境同步（Windows 注册表）
    // -----------------------------------------------------------------------

    /// 将当前激活的 工具 同步到用户环境（Windows 注册表 PATH / Unix 仅链接）
    /// `pub(super)`：被 tool 域（uninstall / switch / unuse）与 apply_global_env 共用。
    pub(super) fn sync_user_env(&self, chain: &ConfigChain) -> Result<()> {
        #[cfg(windows)]
        {
            let mut paths_to_add: Vec<String> = Vec::new();
            for (name, version) in chain.active_tools() {
                let Ok(tool) = self.lookup_tool(&name) else { continue };
                let v = envhive_toolkit::tool::version::Version::new(version.clone());
                if !tool.is_installed(&self.paths, &v) {
                    continue;
                }
                // 链接须真实有效才写 PATH：坏链接（如空目录残留）指向的路径不存在，写入无意义
                if !envhive_core::env::symlink::read_link(&tool.current_link(&self.paths)).is_ok() {
                    tracing::warn!("[{name}] current 链接无效，跳过 PATH 写入");
                    continue;
                }
                let bin = tool.bin_dir(&self.paths);
                let bin_str = bin.to_string_lossy().trim_end_matches(['\\', '/']).to_string();
                if !paths_to_add.iter().any(|p| p.eq_ignore_ascii_case(&bin_str)) {
                    paths_to_add.push(bin_str);
                }
                // 环境变量（JAVA_HOME 等）
                let desc = tool.desc();
                for (k, _kind) in desc.env_vars() {
                    let link = tool.current_link(&self.paths);
                    let val = link.to_string_lossy().trim_end_matches(['\\', '/']).to_string();
                    if let Err(e) = registry_windows::set_user_env_var(k, &val, true) {
                        tracing::warn!("写入用户环境变量 {k} 失败: {e}");
                    }
                }
                // 插件 env_vars（{root} 占位符）
                let extra = tool.extra_env_keys(&self.paths, true);
                for (k, v) in &extra.vars {
                    if let Some(val) = v {
                        if let Err(e) = registry_windows::set_user_env_var(k, val, true) {
                            tracing::warn!("写入插件环境变量 {k} 失败: {e}");
                        }
                    }
                }
            }
            let prefix = self.paths.tools.to_string_lossy().to_string();
            if let Err(e) = registry_windows::update_user_path(&paths_to_add, &prefix) {
                tracing::warn!("更新用户 PATH 失败: {e}");
            }
            registry_windows::broadcast_env_change();
        }
        #[cfg(not(windows))]
        {
            tracing::info!("Unix 平台：已重建链接，shell profile 注入计划于 P1 完成");
        }
        Ok(())
    }

    /// 公开包装：导入环境 / 模板应用后同步用户环境
    pub fn sync_user_env_pub(&self, chain: &ConfigChain) -> Result<()> {
        self.sync_user_env(chain)
    }

    // -----------------------------------------------------------------------
    // 环境预览与进程注入（roadmap F：Session 落地）
    // -----------------------------------------------------------------------

    /// 计算当前链上合并的 Envs（Global + Project 工具版本 + 全局 env）
    pub fn resolve_envs(&self, project_dir: Option<&Path>) -> Result<Envs> {
        let chain = self.chain(project_dir)?;
        let global_env = self.config.lock().unwrap().env_global().clone();
        envhive_toolkit::resolve_envs(&self.paths, &chain, &global_env, &self.lookup())
    }

    /// 进程注入（roadmap F：以此环境启动子进程）
    /// 注意：此场景**不隐藏**子进程控制台 —— 用户要看到终端 / 服务器输出（cmd / node / code 等）。
    /// （安装验证等场景的 hide_console 在 install 管道内单独使用，与这里无关。）
    pub fn spawn_with_env(&self, project_dir: Option<&Path>, command: &str, args: &[String]) -> Result<()> {
        let envs = self.resolve_envs(project_dir)?;
        let map = envs.as_map(); // 仅用于下方「孤儿变量」判断（contains_key）
        let mut cmd = std::process::Command::new(command);
        cmd.args(args);
        // 不要调用 hide_console：否则 Windows 上 cmd 等控制台程序会被 CREATE_NO_WINDOW
        // 静默启动，用户看不到任何窗口（表现为"点了没反应"）。
        // 用 apply_to_command 注入：set 用 env 覆盖，unset（None）用 env_remove 真正删除，
        // 否则子进程会保留父环境里的旧值，导致 Envs::unset 失效。
        envs.apply_to_command(&mut cmd);
        // 清除继承环境中的"孤儿"工具 根变量：指向已删除目录的残留值（如卸载其他版本管理器
        // 后遗留的 GOROOT/GOPATH）会让子进程里的 go/java 等报错。envhive 已注入的优先，
        // 仅在值非空、目标路径不存在、且未被本环境覆盖时移除。
        const TOOL_ROOT_VARS: &[&str] = &[
            "GOROOT", "GOPATH", "JAVA_HOME", "PYTHON_HOME", "ANDROID_HOME", "NDK_HOME",
            "RUSTUP_HOME", "CARGO_HOME", "NVM_HOME", "NVM_SYMLINK",
        ];
        for key in TOOL_ROOT_VARS {
            if map.contains_key(*key) {
                continue;
            }
            if let Ok(val) = std::env::var(key) {
                let trimmed = val.trim();
                if !trimmed.is_empty() {
                    let p = std::path::Path::new(trimmed);
                    if !p.exists() {
                        cmd.env_remove(key);
                        tracing::info!("spawn_with_env: 清除孤儿环境变量 {key}={trimmed}（路径不存在）");
                    }
                }
            }
        }
        if let Some(dir) = project_dir {
            cmd.current_dir(dir);
        }
        let child = cmd.spawn().map_err(|e| {
            EnvHiveError::with_source(EnvHiveErrorKind::Install, format!("启动 {command} 失败"), e)
        })?;
        tracing::info!("已以 envhive 环境启动 {command}（pid={}）", child.id());
        Ok(())
    }

    // -----------------------------------------------------------------------
    // UI v2：应用为系统环境变量（写系统变量，非会话注入）
    // -----------------------------------------------------------------------

    /// 把当前全局链（工具 注入 + config [env]）写入系统变量（Windows 用户注册表）并广播。
    /// 已打开的窗口不生效，需重开新终端 —— 由前端提示。
    pub fn apply_global_env(&self) -> Result<super::ApplyGlobalResult> {
        let chain = self.chain(None)?;
        // 1. 工具 注入：PATH 前缀 + JAVA_HOME 等（复用切换全局时的同步逻辑）
        self.sync_user_env(&chain)?;
        // 2. config [env] 全局变量并入
        #[cfg(windows)]
        {
            let globals = self.config.lock().unwrap().env_global().clone();
            for (k, v) in &globals {
                if let Err(e) = registry_windows::set_user_env_var(k, v, true) {
                    tracing::warn!("写入全局环境变量 {k} 失败: {e}");
                }
            }
            envhive_core::env::registry_windows::broadcast_env_change();
        }
        #[cfg(not(windows))]
        {
            tracing::info!("Unix：全局变量由 shell profile 注入，apply_global_env 仅同步链接");
        }
        Ok(super::ApplyGlobalResult {
            applied: true,
            message: "已写入系统环境变量（新开的终端生效，已打开窗口需重启）".into(),
        })
    }
}
