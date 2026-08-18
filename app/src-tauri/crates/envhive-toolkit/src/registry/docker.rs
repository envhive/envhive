//! Docker 镜像加速（P2）：`~/.docker/config.json` 的 `registry-mirrors` 数组。
//! Docker 客户端启动时读取该数组作为镜像加速器；official 预设 = 清空数组回 Docker Hub 默认。

use std::path::PathBuf;

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::registry::{
    atomic_write_file, backup_file, preset_clears, RegistryPreset, RegistryState, ToolRegistryWriter,
};

pub struct DockerWriter;

impl DockerWriter {
    fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Io, "无法确定主目录"))?;
        Ok(home.join(".docker").join("config.json"))
    }
}

impl ToolRegistryWriter for DockerWriter {
    fn tool_name(&self) -> &str {
        "docker"
    }

    fn config_path(&self) -> Result<PathBuf> {
        Self::config_path()
    }

    fn read_current(&self) -> Result<RegistryState> {
        let path = Self::config_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        // registry-mirrors 数组第一个
        let mirrors = content
            .find("\"registry-mirrors\"")
            .and_then(|i| {
                let rest = &content[i..];
                let open = rest.find('[')?;
                let close = rest[open..].find(']')? + open;
                Some(&rest[open + 1..close])
            })
            .map(|s| s.split(',').filter_map(|p| p.trim().trim_matches('"').split('"').next()).collect::<Vec<_>>())
            .unwrap_or_default();
        let current_url = mirrors.first().map(|s| s.to_string());
        let preset_name = current_url
            .as_deref()
            .and_then(|u| crate::registry::all_presets().iter().find(|p| p.tool == "docker" && u.contains(p.url.as_str())).map(|p| p.name.to_string()));
        Ok(RegistryState {
            tool: "docker".into(),
            current_url,
            config_file: Some(path.to_string_lossy().to_string()),
            preset_name,
        })
    }

    fn apply(&self, preset: &RegistryPreset) -> Result<()> {
        let path = Self::config_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        backup_file(&path)?;

        // 解析为 JSON（尽量保留原格式；解析失败则重建最小结构）
        let mut json: serde_json::Value = serde_json::from_str(&content).unwrap_or_else(|_| {
            serde_json::json!({ "auths": {} })
        });
        let mirrors = if preset_clears(preset) {
            Vec::new()
        } else {
            vec![serde_json::Value::String(preset.url.to_string())]
        };
        json["registry-mirrors"] = serde_json::Value::Array(mirrors);
        let merged = serde_json::to_string_pretty(&json).unwrap_or_else(|_| content);
        atomic_write_file(&path, &merged)?;
        Ok(())
    }
}
