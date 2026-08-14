//! cargo 镜像：`~/.cargo/config.toml`
//! ```toml
//! [source.crates-io]
//! replace-with = "rsproxy"
//!
//! [source.rsproxy]
//! registry = "sparse+https://rsproxy.cn/index/"
//! ```
//! 精确 merge：只改 `[source.crates-io]` 的 replace-with 与目标 `[source.<name>]` 段，保留其他内容。

use std::path::PathBuf;

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::registry::{backup_file, RegistryPreset, RegistryState, ToolRegistryWriter};

pub struct CargoWriter;

impl CargoWriter {
    fn config_path_impl() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Io, "无法确定主目录"))?;
        Ok(home.join(".cargo").join("config.toml"))
    }
}

impl ToolRegistryWriter for CargoWriter {
    fn tool_name(&self) -> &str {
        "cargo"
    }

    fn config_path(&self) -> Result<PathBuf> {
        Self::config_path_impl()
    }

    fn read_current(&self) -> Result<RegistryState> {
        let path = Self::config_path_impl()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let replace_with = parse_replace_with(&content);
        let current_url = replace_with.and_then(|name| parse_source_registry(&content, &name));
        let preset_name = current_url
            .as_deref()
            .and_then(|u| crate::registry::all_presets().iter().find(|p| p.tool == "cargo" && (u.contains(p.url.as_str()) || p.url.contains(u))).map(|p| p.name.to_string()));
        Ok(RegistryState {
            tool: "cargo".into(),
            current_url,
            config_file: Some(path.to_string_lossy().to_string()),
            preset_name,
        })
    }

    fn apply(&self, preset: &RegistryPreset) -> Result<()> {
        let path = Self::config_path_impl()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        backup_file(&path)?;

        let sparse = preset
            .extra
            .iter()
            .find(|(k, _)| k == "sparse")
            .map(|(_, v)| v.as_str())
            .unwrap_or(preset.url.as_str());

        let merged = merge_cargo_config(&content, &preset.name, &preset.url, sparse);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, merged)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

fn parse_replace_with(content: &str) -> Option<String> {
    let mut in_crates_io = false;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("[source.crates-io]") {
            in_crates_io = true;
            continue;
        }
        if in_crates_io && t.starts_with('[') {
            in_crates_io = false;
        }
        if in_crates_io {
            if let Some(v) = t.strip_prefix("replace-with") {
                let v = v.trim().trim_start_matches('=').trim().trim_matches('"').trim().to_string();
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn parse_source_registry(content: &str, source: &str) -> Option<String> {
    let marker = format!("[source.{source}]");
    let mut in_src = false;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with(&marker) {
            in_src = true;
            continue;
        }
        if in_src && t.starts_with('[') {
            in_src = false;
        }
        if in_src {
            if let Some(v) = t.strip_prefix("registry") {
                return Some(v.trim().trim_start_matches('=').trim().trim_matches('"').trim().to_string());
            }
        }
    }
    None
}

fn merge_cargo_config(content: &str, mirror: &str, index_url: &str, sparse_url: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut replaced = false;
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    // 1) 更新 [source.crates-io] 的 replace-with
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim().to_string();
        if t == "[source.crates-io]" {
            out.push(lines[i].clone());
            i += 1;
            // 消费该段内行
            while i < lines.len() {
                let inner = lines[i].trim().to_string();
                if inner.starts_with('[') {
                    break;
                }
                if inner.starts_with("replace-with") {
                    out.push(format!("replace-with = \"{mirror}\""));
                    replaced = true;
                } else {
                    out.push(lines[i].clone());
                }
                i += 1;
            }
            if !replaced {
                out.push(format!("replace-with = \"{mirror}\""));
            }
            continue;
        }
        out.push(lines[i].clone());
        i += 1;
    }

    // 2) 更新/追加 [source.<mirror>] 段
    let marker = format!("[source.{mirror}]");
    let mut has_mirror = false;
    let mut result: Vec<String> = Vec::new();
    i = 0;
    while i < out.len() {
        let t = out[i].trim().to_string();
        if t == marker {
            has_mirror = true;
            result.push(out[i].clone());
            i += 1;
            while i < out.len() {
                let inner = out[i].trim().to_string();
                if inner.starts_with('[') {
                    break;
                }
                if inner.starts_with("registry") {
                    result.push(format!("registry = \"{sparse_url}\""));
                } else {
                    result.push(out[i].clone());
                }
                i += 1;
            }
            if !result.iter().any(|l| l.trim().starts_with("registry")) {
                result.push(format!("registry = \"{sparse_url}\""));
            }
            continue;
        }
        result.push(out[i].clone());
        i += 1;
    }
    if !has_mirror {
        // 追加到文件末尾
        result.push(String::new());
        result.push(marker.clone());
        result.push(format!("registry = \"{sparse_url}\""));
    }

    // 确保存在 [source.crates-io] 段并带 replace-with。
    // 关键修复：若原配置根本没有 [source.crates-io]（全新机器 / 干净 config.toml），
    // 前面的逻辑不会写入该段，导致 replace-with 缺失、cargo 仍直连 crates.io，镜像静默无效。
    if !result.iter().any(|l| l.trim() == "[source.crates-io]") {
        result.push(String::new());
        result.push("[source.crates-io]".to_string());
        result.push(format!("replace-with = \"{mirror}\""));
    }

    let mut text = result.join("\n");
    if !text.ends_with('\n') {
        text.push('\n');
    }
    let _ = index_url; // url 用于 `registry = url` 场景；cargo 统一走 sparse
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cargo_merge_keeps_other_sections() {
        let content = r#"
[build]
jobs = 4

[source.crates-io]
replace-with = "oldmirror"
"#;
        let merged = merge_cargo_config(content, "rsproxy", "https://rsproxy.cn/index", "sparse+https://rsproxy.cn/index/");
        assert!(merged.contains("[build]"));
        assert!(merged.contains("jobs = 4"));
        assert!(merged.contains("replace-with = \"rsproxy\""));
        assert!(merged.contains("[source.rsproxy]"));
        assert!(merged.contains("registry = \"sparse+https://rsproxy.cn/index/\""));
    }

    #[test]
    fn test_cargo_parse_with_spaces() {
        // 回归测试：`replace-with = "rsproxy"` 中 = 前后带空格时，
        // 之前 trim_start_matches('=') 在 trim 之前执行导致解析出垃圾值
        let content = r#"
[source.crates-io]
replace-with = "rsproxy"

[source.rsproxy]
registry = "sparse+https://rsproxy.cn/index/"
"#;
        assert_eq!(parse_replace_with(content).as_deref(), Some("rsproxy"));
        assert_eq!(
            parse_source_registry(content, "rsproxy").as_deref(),
            Some("sparse+https://rsproxy.cn/index/")
        );
    }

    #[test]
    fn test_cargo_parse_no_spaces() {
        let content = "[source.crates-io]\nreplace-with=\"ustc\"\n\n[source.ustc]\nregistry=\"sparse+https://mirrors.ustc.edu.cn/crates.io-index/\"\n";
        assert_eq!(parse_replace_with(content).as_deref(), Some("ustc"));
        assert_eq!(
            parse_source_registry(content, "ustc").as_deref(),
            Some("sparse+https://mirrors.ustc.edu.cn/crates.io-index/")
        );
    }
}
