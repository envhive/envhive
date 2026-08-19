//! Dart pub.dev 镜像（P2）：`~/.pub-cache/config.json` 的 `PUB_HOSTED_URL` 键。
//! Dart 2.19+ 从 `$PUB_CACHE/config.json` 读取该键；亦可配合环境变量注入。

use std::path::PathBuf;

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::registry::{
    atomic_write_file, backup_file, extract_json_string, RegistryPreset, RegistryState, ToolRegistryWriter,
};

pub struct PubWriter;

impl PubWriter {
    fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Io, "无法确定主目录"))?;
        Ok(home.join(".pub-cache").join("config.json"))
    }
}

impl ToolRegistryWriter for PubWriter {
    fn tool_name(&self) -> &str {
        "pub"
    }

    fn config_path(&self) -> Result<PathBuf> {
        Self::config_path()
    }

    fn read_current(&self) -> Result<RegistryState> {
        let path = Self::config_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let url = extract_json_string(&content, "PUB_HOSTED_URL");
        let preset_name = url
            .as_deref()
            .and_then(|u| crate::registry::all_presets().iter().find(|p| p.tool == "pub" && (u.contains(p.url.as_str()) || p.url.contains(u))).map(|p| p.name.to_string()));
        Ok(RegistryState {
            tool: "pub".into(),
            current_url: url,
            config_file: Some(path.to_string_lossy().to_string()),
            preset_name,
        })
    }

    fn apply(&self, preset: &RegistryPreset) -> Result<()> {
        let path = Self::config_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        backup_file(&path)?;

        let mut json: serde_json::Value = serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}));
        // official 预设 → 删除该键（回 pub.dev 默认）
        if preset.name == "official" {
            if let Some(obj) = json.as_object_mut() {
                obj.remove("PUB_HOSTED_URL");
            }
        } else {
            json["PUB_HOSTED_URL"] = serde_json::Value::String(preset.url.to_string());
        }
        let merged = serde_json::to_string_pretty(&json).unwrap_or_else(|_| content);
        atomic_write_file(&path, &merged)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_hosted_url() {
        let json = r#"{ "PUB_HOSTED_URL": "https://mirrors.sjtug.sjtu.edu.cn/dart-pub" }"#;
        assert_eq!(extract_json_string(json, "PUB_HOSTED_URL").as_deref(), Some("https://mirrors.sjtug.sjtu.edu.cn/dart-pub"));
    }
}
