//! conda 镜像（P2）：`~/.condarc` 的 `channels:` 列表。
//! 镜像预设写 channels = [main, free]（extra channel2 提供 free）；official 预设删除 channels 回 defaults。

use std::path::PathBuf;

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::registry::{
    atomic_write_file, backup_file, extract_yaml_list, preset_clears, replace_yaml_list, RegistryPreset,
    RegistryState, ToolRegistryWriter,
};

pub struct CondaWriter;

impl CondaWriter {
    fn condarc_path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Io, "无法确定主目录"))?;
        Ok(home.join(".condarc"))
    }
}

impl ToolRegistryWriter for CondaWriter {
    fn tool_name(&self) -> &str {
        "conda"
    }

    fn config_path(&self) -> Result<PathBuf> {
        Self::condarc_path()
    }

    fn read_current(&self) -> Result<RegistryState> {
        let path = Self::condarc_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let url = extract_yaml_list(&content, "channels").and_then(|v| v.first().cloned());
        let preset_name = url
            .as_deref()
            .and_then(|u| crate::registry::all_presets().iter().find(|p| p.tool == "conda" && (u.contains(p.url.as_str()) || p.url.contains(u))).map(|p| p.name.to_string()));
        Ok(RegistryState {
            tool: "conda".into(),
            current_url: url,
            config_file: Some(path.to_string_lossy().to_string()),
            preset_name,
        })
    }

    fn apply(&self, preset: &RegistryPreset) -> Result<()> {
        let path = Self::condarc_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        backup_file(&path)?;

        let values: Vec<String> = if preset_clears(preset) {
            Vec::new()
        } else {
            let mut v = vec![preset.url.to_string()];
            for (k, val) in &preset.extra {
                if k.as_str() == "channel2" {
                    v.push(val.to_string());
                }
            }
            v
        };
        let merged = replace_yaml_list(&content, "channels", &values);
        atomic_write_file(&path, &merged)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_channels() {
        let yaml = "channels:\n  - defaults\n";
        let out = replace_yaml_list(
            yaml,
            "channels",
            &[
                "https://mirrors.tuna.tsinghua.edu.cn/anaconda/pkgs/main".into(),
                "https://mirrors.tuna.tsinghua.edu.cn/anaconda/pkgs/free".into(),
            ],
        );
        assert!(out.contains("mirrors.tuna.tsinghua.edu.cn/anaconda/pkgs/main"));
        assert!(out.contains("mirrors.tuna.tsinghua.edu.cn/anaconda/pkgs/free"));
        assert!(!out.contains("defaults"));
    }

    #[test]
    fn test_clear_channels() {
        let yaml = "channels:\n  - https://mirrors.example\nshow_channel_urls: true\n";
        let out = replace_yaml_list(yaml, "channels", &[]);
        assert!(!out.contains("channels"));
        assert!(out.contains("show_channel_urls: true"));
    }
}
