//! npm（~/.npmrc key=value）与 pip（~/.pip/pip.conf 或 %APPDATA%\pip\pip.ini）镜像写入。
//! 写入安全：读原文 → 备份 → 精确 merge（只改目标键，保留注释）→ 原子写回。

use std::path::PathBuf;

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::registry::{backup_file, RegistryPreset, RegistryState, ToolRegistryWriter};

// ---------------------------------------------------------------------------
// npm
// ---------------------------------------------------------------------------

pub struct NpmWriter;

impl NpmWriter {
    fn npmrc_path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Io, "无法确定主目录"))?;
        Ok(home.join(".npmrc"))
    }
}

impl ToolRegistryWriter for NpmWriter {
    fn tool_name(&self) -> &str {
        "npm"
    }

    fn config_path(&self) -> Result<PathBuf> {
        Self::npmrc_path()
    }

    fn read_current(&self) -> Result<RegistryState> {
        let path = Self::npmrc_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let registry = content
            .lines()
            .find_map(|l| l.trim().strip_prefix("registry=").map(|v| v.trim().to_string()));
        let preset_name = registry
            .as_deref()
            .and_then(|u| crate::registry::all_presets().iter().find(|p| p.tool == "npm" && (u.contains(p.url.as_str()) || p.url.contains(u))).map(|p| p.name.to_string()));
        Ok(RegistryState {
            tool: "npm".into(),
            current_url: registry,
            config_file: Some(path.to_string_lossy().to_string()),
            preset_name,
        })
    }

    fn apply(&self, preset: &RegistryPreset) -> Result<()> {
        let path = Self::npmrc_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        backup_file(&path)?;

        let mut updates: Vec<(&str, &str)> = vec![("registry", preset.url.as_str())];
        for (k, v) in &preset.extra {
            updates.push((k.as_str(), v.as_str()));
        }
        let merged = merge_ini(&content, &updates, &[]);
        atomic_write(&path, &merged)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// pip
// ---------------------------------------------------------------------------

pub struct PipWriter;

impl PipWriter {
    fn pip_conf_path() -> Result<PathBuf> {
        #[cfg(windows)]
        {
            let appdata = std::env::var("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join("AppData").join("Roaming"));
            Ok(appdata.join("pip").join("pip.ini"))
        }
        #[cfg(not(windows))]
        {
            let home = dirs::home_dir().ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Io, "无法确定主目录"))?;
            Ok(home.join(".pip").join("pip.conf"))
        }
    }
}

impl ToolRegistryWriter for PipWriter {
    fn tool_name(&self) -> &str {
        "pip"
    }

    fn config_path(&self) -> Result<PathBuf> {
        Self::pip_conf_path()
    }

    fn read_current(&self) -> Result<RegistryState> {
        let path = Self::pip_conf_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let index_url = content
            .lines()
            .find_map(|l| l.trim().strip_prefix("index-url=").map(|v| v.trim().to_string()));
        let preset_name = index_url
            .as_deref()
            .and_then(|u| crate::registry::all_presets().iter().find(|p| p.tool == "pip" && (u.contains(p.url.as_str()) || p.url.contains(u))).map(|p| p.name.to_string()));
        Ok(RegistryState {
            tool: "pip".into(),
            current_url: index_url,
            config_file: Some(path.to_string_lossy().to_string()),
            preset_name,
        })
    }

    fn apply(&self, preset: &RegistryPreset) -> Result<()> {
        let path = Self::pip_conf_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        backup_file(&path)?;

        // pip 配置在 [global] 段内
        let mut updates: Vec<(&str, &str)> = vec![("index-url", preset.url.as_str())];
        for (k, v) in &preset.extra {
            updates.push((k.as_str(), v.as_str()));
        }
        let merged = merge_ini_section(&content, "global", &updates, &[]);
        atomic_write(&path, &merged)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 通用 INI（key=value 子集）工具
// ---------------------------------------------------------------------------

/// 顶层 key=value merge：替换已有键，追加未出现的键，保留注释与空行
pub(crate) fn merge_ini(content: &str, updates: &[(&str, &str)], removes: &[&str]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        let key = trimmed
            .split_once('=')
            .map(|(k, _)| k.trim())
            .unwrap_or("");
        if key.starts_with('[') || key.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            out.push(line.to_string());
            continue;
        }
        if let Some((_, v)) = updates.iter().find(|(k, _)| *k == key) {
            out.push(format!("{key}={v}"));
            seen.push(key);
        } else if removes.contains(&key) {
            // 跳过（删除）
        } else {
            out.push(line.to_string());
        }
    }
    for (k, v) in updates {
        if !seen.contains(k) {
            out.push(format!("{k}={v}"));
        }
    }
    out.join("\n") + if out.is_empty() { "" } else { "\n" }
}

/// 指定 section（如 [global]）内的 key=value merge；section 外键保留
pub(crate) fn merge_ini_section(
    content: &str,
    section: &str,
    updates: &[(&str, &str)],
    removes: &[&str],
) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_section = false;
    let mut seen: Vec<&str> = Vec::new();
    let mut has_section = false;

    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let name = trimmed.trim_start_matches('[').trim_end_matches(']').trim().to_ascii_lowercase();
            if name == section.to_ascii_lowercase() {
                in_section = true;
                has_section = true;
                out.push(line.to_string());
                continue;
            } else {
                // 离开目标 section：把 section 内未出现的键补写到此处之前（避免落到后续段）
                if in_section {
                    for (k, v) in updates {
                        if !seen.contains(k) {
                            out.push(format!("{k}={v}"));
                        }
                    }
                    seen.clear();
                }
                in_section = false;
                out.push(line.to_string());
                continue;
            }
        }
        if !in_section {
            out.push(line.to_string());
            continue;
        }
        let key = trimmed.split_once('=').map(|(k, _)| k.trim()).unwrap_or("");
        if trimmed.starts_with('#') || trimmed.starts_with(';') || key.is_empty() {
            out.push(line.to_string());
            continue;
        }
        if let Some((_, v)) = updates.iter().find(|(k, _)| *k == key) {
            out.push(format!("{key}={v}"));
            seen.push(key);
        } else if removes.contains(&key) {
            // 删除
        } else {
            out.push(line.to_string());
        }
    }

    // 文件结束：若目标 section 是最后一个（仍在段内），补写未出现的键
    if in_section {
        for (k, v) in updates {
            if !seen.contains(k) {
                out.push(format!("{k}={v}"));
            }
        }
    }

    // 没有目标 section 则新建
    if !has_section {
        out.push(format!("\n[{section}]"));
    }
    for (k, v) in updates {
        if !seen.contains(k) {
            out.push(format!("{k}={v}"));
        }
    }
    out.join("\n") + "\n"
}

/// 原子写回：临时文件 + rename
fn atomic_write(path: &PathBuf, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_ini_keeps_comments() {
        let content = "# my comment\nregistry=https://old.example\nproxy=http://p:8080\n\n";
        let merged = merge_ini(content, &[("registry", "https://new.example")], &[]);
        assert!(merged.contains("# my comment"));
        assert!(merged.contains("registry=https://new.example"));
        assert!(merged.contains("proxy=http://p:8080"));
    }

    #[test]
    fn test_merge_ini_section() {
        let content = "[global]\nindex-url=https://old\n[install]\ntrusted-host=x\n";
        let merged = merge_ini_section(content, "global", &[("index-url", "https://new")], &[]);
        assert!(merged.contains("index-url=https://new"));
        assert!(merged.contains("[install]"));
        assert!(merged.contains("trusted-host=x"));
    }
}
