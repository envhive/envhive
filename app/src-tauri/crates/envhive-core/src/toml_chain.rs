//! 三作用域配置链（roadmap B，对应 vfox env/vfox_toml_chain）
//! - Global：`~/.envhive/.envhive.toml`
//! - Project：`<project>/.envhive.toml`（由工作目录向上定位）
//! - Session：`~/.envhive/tmp/<date>-<pid>/config.toml`（MVP 预留，进程注入模式实现）
//!
//! 合并语义：`Global < Session < Project`（后者优先）；TOML 弹性反序列化（untagged）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::pathmeta::PathMeta;
use crate::util::safe_component;

/// `[tools]` 表项：`nodejs = "22.11.0"` 或 `java = { version = "21", vendor = "openjdk", unlink = true }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolValue {
    Plain(String),
    Attrs {
        version: String,
        vendor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unlink: Option<bool>,
    },
}

impl ToolValue {
    pub fn version(&self) -> &str {
        match self {
            ToolValue::Plain(v) => v,
            ToolValue::Attrs { version, .. } => version,
        }
    }

    pub fn unlink(&self) -> bool {
        match self {
            ToolValue::Plain(_) => false,
            ToolValue::Attrs { unlink, .. } => unlink.unwrap_or(false),
        }
    }

    pub fn plain(version: impl Into<String>) -> Self {
        ToolValue::Plain(version.into())
    }
}

/// 单个作用域的配置内容（`.envhive.toml`）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScopeConfig {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tools: HashMap<String, ToolValue>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

impl ScopeConfig {
    pub fn from_file(path: &Path) -> Result<ScopeConfig> {
        if !path.exists() {
            return Ok(ScopeConfig::default());
        }
        let content = std::fs::read_to_string(path)?;
        if content.trim().is_empty() {
            return Ok(ScopeConfig::default());
        }
        let cfg: ScopeConfig = toml::from_str(&content).map_err(|e| {
            EnvHiveError::with_source(
                EnvHiveErrorKind::Config,
                format!("解析 {} 失败", path.display()),
                e,
            )
        })?;
        Ok(cfg)
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self)
            .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Config, "序列化配置失败", e))
    }

    /// 原子写回（先写临时文件再改名）
    pub fn save_to(&self, path: &Path) -> Result<()> {
        let content = self.to_toml()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// 从工作目录向上定位项目作用域配置（roadmap B：由当前工作目录向上定位）
/// 优先 `.envhive.toml`，兼容旧版 `.envhive.toml`（品牌迁移前遗留的项目文件）。
pub fn find_project_toml(start_dir: &Path) -> Option<PathBuf> {
    let mut dir = Some(start_dir.to_path_buf());
    while let Some(d) = dir {
        for name in [".envhive.toml", ".envhive.toml"] {
            let candidate = d.join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
    None
}

/// 配置链：Global + Project (+ Session)
pub struct ConfigChain {
    pub global_path: PathBuf,
    pub global: ScopeConfig,
    pub project_path: Option<PathBuf>,
    pub project: Option<ScopeConfig>,
    pub session: Option<ScopeConfig>,
}

impl ConfigChain {
    pub fn load(paths: &PathMeta, project_dir: Option<&Path>) -> Result<ConfigChain> {
        let global = ScopeConfig::from_file(&paths.global_toml)?;

        let (project_path, project) = match project_dir {
            Some(dir) => match find_project_toml(dir) {
                Some(p) => (Some(p.clone()), Some(ScopeConfig::from_file(&p)?)),
                None => (None, None),
            },
            None => (None, None),
        };

        Ok(ConfigChain { global_path: paths.global_toml.clone(), global, project_path, project, session: None })
    }

    /// 合并后的 tools 映射（Global < Session < Project，后者优先）
    pub fn merged_tools(&self) -> HashMap<String, ToolValue> {
        let mut merged = self.global.tools.clone();
        if let Some(s) = &self.session {
            for (k, v) in &s.tools {
                merged.insert(k.clone(), v.clone());
            }
        }
        if let Some(p) = &self.project {
            for (k, v) in &p.tools {
                merged.insert(k.clone(), v.clone());
            }
        }
        merged
    }

    /// 查询某 工具 的配置值（含版本与 unlink 标记；Project > Session > Global）
    pub fn tool(&self, tool: &str) -> Option<&ToolValue> {
        if let Some(p) = &self.project {
            if let Some(v) = p.tools.get(tool) {
                return Some(v);
            }
        }
        if let Some(s) = &self.session {
            if let Some(v) = s.tools.get(tool) {
                return Some(v);
            }
        }
        self.global.tools.get(tool)
    }

    /// 设置 Global scope 的某 工具 版本并写回
    pub fn set_global_tool(&mut self, tool: &str, value: ToolValue) -> Result<()> {
        if !safe_component(tool) {
            return Err(EnvHiveError::new(EnvHiveErrorKind::Config, format!("非法 工具 名称: {tool}")));
        }
        self.global.tools.insert(tool.to_string(), value);
        self.global.save_to(&self.global_path)
    }

    /// 清除 Global scope 的某 工具 并写回
    pub fn clear_global_tool(&mut self, tool: &str) -> Result<()> {
        self.global.tools.remove(tool);
        self.global.save_to(&self.global_path)
    }

    /// 已设置且版本已安装的 工具 集合（供 PATH 重建）。
    /// 注意：跳过 `unlink = true` 的 工具——其版本仍记录为「激活」，但不注入 PATH
    ///（避免与系统/其他版本管理器的同名工具冲突；需用时通过 `envhive run` 等显式调用）。
    pub fn active_tools(&self) -> Vec<(String, String)> {
        self.merged_tools()
            .into_iter()
            .filter(|(_, v)| !v.unlink())
            .map(|(k, v)| (k, v.version().to_string()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_untagged_tool_value() {
        let toml_str = r#"
[tools]
nodejs = "22.11.0"
java = { version = "21", vendor = "openjdk" }
golang = { version = "1.24.1", unlink = true }
"#;
        let cfg: ScopeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.tools.get("nodejs").unwrap().version(), "22.11.0");
        assert_eq!(cfg.tools.get("java").unwrap().version(), "21");
        assert!(cfg.tools.get("golang").unwrap().unlink());
    }
}
