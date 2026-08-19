//! NuGet 镜像（P2）：`NuGet.Config` 的 `<packageSources>` 段。
//! 管理第一个 `<add key=... value=...>` 的 value 属性；保留其余配置。

use std::path::PathBuf;

use envhive_core::error::Result;
use crate::registry::{
    atomic_write_file, backup_file, RegistryPreset, RegistryState, ToolRegistryWriter,
};

pub struct NugetWriter;

impl NugetWriter {
    fn config_path() -> Result<PathBuf> {
        #[cfg(windows)]
        {
            let appdata = std::env::var("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join("AppData").join("Roaming"));
            Ok(appdata.join("NuGet").join("NuGet.Config"))
        }
        #[cfg(not(windows))]
        {
            use envhive_core::error::{EnvHiveError, EnvHiveErrorKind};
            let home = dirs::home_dir().ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Io, "无法确定主目录"))?;
            Ok(home.join(".nuget").join("NuGet").join("NuGet.Config"))
        }
    }
}

impl ToolRegistryWriter for NugetWriter {
    fn tool_name(&self) -> &str {
        "nuget"
    }

    fn config_path(&self) -> Result<PathBuf> {
        Self::config_path()
    }

    fn read_current(&self) -> Result<RegistryState> {
        let path = Self::config_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let url = first_source_value(&content);
        let preset_name = url
            .as_deref()
            .and_then(|u| crate::registry::all_presets().iter().find(|p| p.tool == "nuget" && (u.contains(p.url.as_str()) || p.url.contains(u))).map(|p| p.name.to_string()));
        Ok(RegistryState {
            tool: "nuget".into(),
            current_url: url,
            config_file: Some(path.to_string_lossy().to_string()),
            preset_name,
        })
    }

    fn apply(&self, preset: &RegistryPreset) -> Result<()> {
        let path = Self::config_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        backup_file(&path)?;
        let merged = merge_sources_xml(&content, &preset.name, &preset.url);
        atomic_write_file(&path, &merged)?;
        Ok(())
    }
}

/// 提取第一个 `<add ... value="...">` 的值
fn first_source_value(content: &str) -> Option<String> {
    content.find("<add").and_then(|i| {
        let tag_end = content[i..].find('>').map(|j| i + j)?;
        let tag = &content[i..tag_end];
        let vi = tag.find("value=")?;
        let rest = &tag[vi + "value=".len()..];
        let rest = rest.trim_start().strip_prefix('"')?;
        rest.split('"').next().map(|s| s.to_string())
    })
}

/// 注入 / 替换 packageSources 中的第一个源
fn merge_sources_xml(content: &str, key: &str, url: &str) -> String {
    let source_line = format!(
        "<add key=\"{key}\" value=\"{url}\" protocolVersion=\"3\" />"
    );
    let has_sources = content.contains("<packageSources>");
    if !has_sources {
        let insert = format!("  <packageSources>\n    {source_line}\n  </packageSources>\n");
        return match content.rfind("</configuration>") {
            Some(i) => format!("{}{}{}", &content[..i], insert, &content[i..]),
            None => format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<configuration>\n{insert}</configuration>\n"
            ),
        };
    }
    // 替换第一个 <add ...> 标签（保留其他属性键）
    let start = content.find("<packageSources>").unwrap();
    let inner = &content[start..];
    if let Some(add_i) = inner.find("<add") {
        let abs = start + add_i;
        let end = content[abs..].find('>').map(|j| abs + j + 1).unwrap_or(abs + 1);
        let merged = format!("{}{}{}", &content[..abs], source_line, &content[end..]);
        // 合并后 packageSources 段可能残留空行，可接受
        return merged;
    }
    // 有段但无 add：插入到段尾
    let seg_end = content.find("</packageSources>").unwrap() + "</packageSources>".len();
    format!("{}{}{}", &content[..seg_end], format!("\n    {source_line}"), &content[seg_end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_value() {
        let xml = r#"<packageSources><add key="nuget.org" value="https://api.nuget.org/v3/index.json" protocolVersion="3" /></packageSources>"#;
        assert_eq!(first_source_value(xml).as_deref(), Some("https://api.nuget.org/v3/index.json"));
    }

    #[test]
    fn test_replace_source() {
        let xml = r#"<configuration><packageSources><add key="old" value="https://old" /></packageSources></configuration>"#;
        let out = merge_sources_xml(xml, "tuna", "https://mirrors.tuna.tsinghua.edu.cn/nuget/v3/index.json");
        assert!(!out.contains("https://old"));
        assert!(out.contains("mirrors.tuna.tsinghua.edu.cn"));
    }

    #[test]
    fn test_insert_sources_section() {
        let xml = "<configuration></configuration>";
        let out = merge_sources_xml(xml, "tuna", "https://mirrors.tuna.tsinghua.edu.cn/nuget/v3/index.json");
        assert!(out.contains("<packageSources>"));
        assert!(out.contains("mirrors.tuna.tsinghua.edu.cn"));
    }
}
