//! maven 镜像：`~/.m2/settings.xml`
//! 管理 `<mirrors>` 段：读取原文 → 替换/插入 mirror 块 → 保留其余 XML。

use std::path::PathBuf;

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::registry::{backup_file, RegistryPreset, RegistryState, ToolRegistryWriter};

pub struct MavenWriter;

impl MavenWriter {
    fn settings_path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Io, "无法确定主目录"))?;
        Ok(home.join(".m2").join("settings.xml"))
    }
}

impl ToolRegistryWriter for MavenWriter {
    fn tool_name(&self) -> &str {
        "maven"
    }

    fn config_path(&self) -> Result<PathBuf> {
        Self::settings_path()
    }

    fn read_current(&self) -> Result<RegistryState> {
        let path = Self::settings_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let url = extract_first_mirror_url(&content);
        let preset_name = url
            .as_deref()
            .and_then(|u| crate::registry::all_presets().iter().find(|p| p.tool == "maven" && (u.contains(p.url.as_str()) || p.url.contains(u))).map(|p| p.name.to_string()));
        Ok(RegistryState {
            tool: "maven".into(),
            current_url: url,
            config_file: Some(path.to_string_lossy().to_string()),
            preset_name,
        })
    }

    fn apply(&self, preset: &RegistryPreset) -> Result<()> {
        let path = Self::settings_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        backup_file(&path)?;
        let merged = merge_settings_xml(&content, &preset.name, &preset.url);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, merged)?;
        Ok(())
    }
}

fn extract_first_mirror_url(content: &str) -> Option<String> {
    // 找 <url>...</url>（第一个镜像）
    content.find("<url>").and_then(|i| {
        let rest = &content[i + 5..];
        rest.find("</url>").map(|j| rest[..j].trim().to_string())
    })
}

/// 在 settings.xml 中注入 / 替换 mirror 块
fn merge_settings_xml(content: &str, mirror_name: &str, url: &str) -> String {
    let mirror_block = format!(
        "<mirror>\n      <id>{mirror_name}</id>\n      <mirrorOf>central</mirrorOf>\n      <url>{url}</url>\n    </mirror>"
    );

    let has_mirrors = content.contains("<mirrors>") && content.contains("</mirrors>");
    if !has_mirrors {
        // 无 <mirrors> 段：插入到 </settings> 前
        let insert = format!(
            "  <mirrors>\n    {mirror_block}\n  </mirrors>\n"
        );
        return match content.rfind("</settings>") {
            Some(i) => format!("{}{}{}", &content[..i], insert, &content[i..]),
            None => format!(
                "<settings xmlns=\"http://maven.apache.org/SETTINGS/1.0.0\">\n  {insert}\n</settings>\n"
            ),
        };
    }

    // 有 <mirrors> 段：整体替换 <mirrors>...</mirrors> 块（<mirrors> 与 </mirrors> 必须成对存在，否则按无段处理）
    let start = content.find("<mirrors>").unwrap();
    let end = content.find("</mirrors>").unwrap() + "</mirrors>".len();
    let inner = format!("<mirrors>\n    {mirror_block}\n  </mirrors>");
    format!("{}{}{}", &content[..start], inner, &content[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_maven_insert() {
        let xml = "<settings>\n  <localRepository>/tmp/repo</localRepository>\n</settings>";
        let out = merge_settings_xml(xml, "aliyun", "https://maven.aliyun.com/repository/central");
        assert!(out.contains("<localRepository>"));
        assert!(out.contains("<mirrors>"));
        assert!(out.contains("maven.aliyun.com"));
        assert!(out.contains("</settings>"));
    }

    #[test]
    fn test_maven_replace() {
        let xml = "<settings><mirrors><mirror><id>old</id><mirrorOf>central</mirrorOf><url>https://old</url></mirror></mirrors></settings>";
        let out = merge_settings_xml(xml, "aliyun", "https://maven.aliyun.com/repository/central");
        assert!(!out.contains("https://old"));
        assert!(out.contains("maven.aliyun.com"));
    }

    #[test]
    fn test_maven_replace_valid_xml() {
        // 回归：替换既有 <mirrors> 段时不可丢弃 </mirrors> 闭合标签，否则 maven 构建直接失败
        let xml = "<settings><mirrors><mirror><id>old</id><mirrorOf>central</mirrorOf><url>https://old</url></mirror></mirrors></settings>";
        let out = merge_settings_xml(xml, "aliyun", "https://maven.aliyun.com/repository/central");
        assert!(out.contains("</mirrors>"), "缺少 </mirrors> 闭合标签: {out}");
        assert!(out.contains("</settings>"), "缺少 </settings> 闭合标签: {out}");
        assert_eq!(out.matches("</mirrors>").count(), 1, "不应出现重复/断裂的 </mirrors>: {out}");
    }
}
