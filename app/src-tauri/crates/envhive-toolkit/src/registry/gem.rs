//! RubyGems 镜像（P2）：`~/.gemrc` 的 `:sources:` 列表。
//! gem 支持多源，蜂巢切换为单源（等同 `gem sources --clear-all` 后添加唯一源）。

use std::path::PathBuf;

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::registry::{
    atomic_write_file, backup_file, extract_yaml_list, replace_yaml_list, RegistryPreset, RegistryState,
    ToolRegistryWriter,
};

pub struct GemWriter;

impl GemWriter {
    fn gemrc_path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Io, "无法确定主目录"))?;
        Ok(home.join(".gemrc"))
    }
}

impl ToolRegistryWriter for GemWriter {
    fn tool_name(&self) -> &str {
        "gem"
    }

    fn config_path(&self) -> Result<PathBuf> {
        Self::gemrc_path()
    }

    fn read_current(&self) -> Result<RegistryState> {
        let path = Self::gemrc_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        // YAML 键为 `:sources:`（Ruby symbol），helper 统一传裸键（内部拼冒号）
        let url = extract_yaml_list(&content, ":sources").and_then(|v| v.first().cloned());
        let preset_name = url
            .as_deref()
            .and_then(|u| crate::registry::all_presets().iter().find(|p| p.tool == "gem" && (u.contains(p.url.as_str()) || p.url.contains(u))).map(|p| p.name.to_string()));
        Ok(RegistryState {
            tool: "gem".into(),
            current_url: url,
            config_file: Some(path.to_string_lossy().to_string()),
            preset_name,
        })
    }

    fn apply(&self, preset: &RegistryPreset) -> Result<()> {
        let path = Self::gemrc_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        backup_file(&path)?;
        let merged = replace_yaml_list(&content, ":sources", &[preset.url.to_string()]);
        atomic_write_file(&path, &merged)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_sources() {
        let yaml = ":backtrace: false\n:sources:\n- https://rubygems.org/\n- https://other.example\n";
        let v = extract_yaml_list(yaml, ":sources").unwrap();
        assert_eq!(v, vec!["https://rubygems.org/", "https://other.example"]);
    }

    #[test]
    fn test_replace_sources() {
        let yaml = ":backtrace: true\n:sources:\n- https://rubygems.org/\n";
        let out = replace_yaml_list(yaml, ":sources", &["https://mirrors.tuna.tsinghua.edu.cn/rubygems/".into()]);
        assert!(out.contains(":backtrace: true"));
        assert!(out.contains("mirrors.tuna.tsinghua.edu.cn"));
        assert!(!out.contains("rubygems.org"));
    }

    #[test]
    fn test_append_sources_when_missing() {
        let yaml = ":backtrace: false\n";
        let out = replace_yaml_list(yaml, ":sources", &["https://mirrors.aliyun.com/rubygems/".into()]);
        assert!(out.contains(":sources:"));
        assert!(out.contains("mirrors.aliyun.com"));
    }
}
