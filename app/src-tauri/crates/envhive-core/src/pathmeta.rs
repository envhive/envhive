//! 路径元数据与目录初始化（roadmap C：对应 vfox internal/pathmeta）
//! 首次运行创建 `~/.envhive/` 完整目录布局，无网络依赖。

use std::path::{Path, PathBuf};

use crate::config::AppConfig;
use crate::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::util::safe_component;

#[derive(Clone)]
pub struct PathMeta {
    /// `~/.envhive` 根目录
    pub root: PathBuf,
    /// `~/.envhive/config.yaml`
    pub config_file: PathBuf,
    /// `~/.envhive/.envhive.toml` —— Global scope 配置
    pub global_toml: PathBuf,
    /// 工具 存储根（真实安装物，平铺不可变；由 config.storage.tool_path 决定）
    pub install_root: PathBuf,
    /// 插件目录（Lua 工具 源）
    pub plugins: PathBuf,
    /// Global scope symlink / junction 目录
    pub tools: PathBuf,
    /// Session 临时目录
    pub tmp: PathBuf,
    /// 日志目录
    pub logs: PathBuf,
    /// 版本列表缓存目录（`installs/versions/<tool>.json`）
    pub versions_cache: PathBuf,
    /// 状态目录（`~/.envhive/state/`，指纹 / 使用统计等）
    pub state: PathBuf,
}

impl PathMeta {
    /// 按数据根目录初始化（root 由调用方决定：默认 `~/.envhive`，品牌迁移失败时可回退旧目录）
    pub fn init(root: &Path, config: &AppConfig) -> Result<PathMeta> {
        // Global scope 配置：优先 `.envhive.toml`，兼容旧版 `.envhive.toml`（品牌迁移前遗留）
        let global_toml = {
            let primary = root.join(".envhive.toml");
            if primary.exists() {
                primary
            } else {
                let legacy = root.join(".envhive.toml");
                if legacy.exists() { legacy } else { primary }
            }
        };

        let install_root = config.tool_root();

        let meta = PathMeta {
            config_file: root.join("config.yaml"),
            global_toml,
            install_root,
            plugins: root.join("plugins"),
            tools: root.join("tools"),
            tmp: root.join("tmp"),
            logs: root.join("logs"),
            versions_cache: root.join("installs").join("versions"),
            state: root.join("state"),
            root: root.to_path_buf(),
        };

        // 目录布局：全部创建（无网络依赖）
        for dir in [
            &meta.install_root,
            &meta.plugins,
            &meta.tools,
            &meta.tmp,
            &meta.logs,
            &meta.versions_cache,
            &meta.state,
        ] {
            std::fs::create_dir_all(dir).map_err(|e| {
                EnvHiveError::with_source(
                    EnvHiveErrorKind::Io,
                    format!("创建目录失败: {}", dir.display()),
                    e,
                )
            })?;
        }

        Ok(meta)
    }

    /// 某 工具 的安装缓存目录：`installs/<tool>/`
    pub fn tool_cache_dir(&self, tool: &str) -> PathBuf {
        self.install_root.join(tool)
    }

    /// 某 工具 某版本的安装目录：`cache/<tool>/v-<version>/`
    pub fn version_dir(&self, tool: &str, version: &str) -> PathBuf {
        self.tool_cache_dir(tool).join(format!("v-{version}"))
    }

    /// Global scope 的 current 链接：`tools/<tool>/current`
    pub fn current_link(&self, tool: &str) -> PathBuf {
        self.tool_link_dir(tool).join("current")
    }

    /// `tools/<tool>/`
    pub fn tool_link_dir(&self, tool: &str) -> PathBuf {
        self.tools.join(tool)
    }

    /// 版本列表缓存文件：`installs/versions/<tool>.json`；发行商维度 → `<tool>-<dist>.json`
    /// （v2：不同发行商 / 是否含 FX 的版本列表天然隔离）
    pub fn versions_cache_file(&self, tool: &str, distribution: Option<&str>) -> PathBuf {
        match distribution {
            Some(d) if !d.is_empty() => self.versions_cache.join(format!("{tool}-{d}.json")),
            _ => self.versions_cache.join(format!("{tool}.json")),
        }
    }
}

/// 校验名称合法（tool / version 不可含路径分隔符）
pub fn ensure_safe_component(kind: &str, name: &str) -> Result<()> {
    if !safe_component(name) {
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::Config,
            format!("非法{kind}名称: {name:?}"),
        ));
    }
    Ok(())
}

/// 供测试 / CLI 复用：按目录重建 PathMeta（不写磁盘）
pub fn from_root(root: &Path) -> PathMeta {
    let install_root = root.join("installs");
    PathMeta {
        root: root.to_path_buf(),
        config_file: root.join("config.yaml"),
        global_toml: root.join(".envhive.toml"),
        install_root: install_root.clone(),
        plugins: root.join("plugins"),
        tools: root.join("tools"),
        tmp: root.join("tmp"),
        logs: root.join("logs"),
        versions_cache: install_root.join("versions"),
        state: root.join("state"),
    }
}
