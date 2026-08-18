//! 版本解析与模糊匹配（roadmap E：精确 → 前缀 → 标签 → 包含）

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::tool::runtime::AvailableVersion;
use crate::tool::version::{compare_versions, is_prefix_match, sort_versions};

/// 标签版本：latest / lts / stable
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveTag {
    Latest,
    Lts,
    Stable,
}

impl ResolveTag {
    pub fn from_str(s: &str) -> Option<ResolveTag> {
        match s.trim().to_ascii_lowercase().as_str() {
            "latest" => Some(ResolveTag::Latest),
            "lts" => Some(ResolveTag::Lts),
            "stable" => Some(ResolveTag::Stable),
            _ => None,
        }
    }
}

/// 带元数据（lts / labels）的版本解析：精确 → 前缀（取最高）→ 标签（用真实 lts/labels）→ 包含匹配。
/// 用于已抓取可用列表的场景：标签解析依赖 AvailableVersion.lts / labels，而非版本字符串里是否出现 "lts" 字样
/// （Adoptium/Temurin 的 LTS 版本如 "21.0.10+7" 字符串中不含 "lts"，旧实现会永远解析失败）。
pub fn resolve_versions_meta<'a>(versions: &'a [AvailableVersion], query: &str) -> Result<&'a str> {
    if versions.is_empty() {
        return Err(EnvHiveError::new(EnvHiveErrorKind::VersionNotFound, "可用版本列表为空"));
    }
    let q = query.trim().trim_start_matches('v');

    // 1. 精确匹配
    if let Some(v) = versions.iter().find(|v| v.version.as_str() == q) {
        return Ok(&v.version);
    }

    // 2. 前缀匹配（收集所有命中，取最高版本）
    let mut hits: Vec<&AvailableVersion> = versions.iter().filter(|v| is_prefix_match(q, &v.version)).collect();
    if !hits.is_empty() {
        hits.sort_by(|a, b| compare_versions(&a.version, &b.version));
        return Ok(&hits.last().unwrap().version);
    }

    // 3. 标签（基于真实元数据）
    if let Some(tag) = ResolveTag::from_str(q) {
        let tag_versions: Vec<&AvailableVersion> = versions.iter().filter(|v| matches_meta(v, tag)).collect();
        if !tag_versions.is_empty() {
            let mut sorted = tag_versions.clone();
            sorted.sort_by(|a, b| compare_versions(&a.version, &b.version));
            return Ok(&sorted.last().unwrap().version);
        }
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::VersionResolve,
            format!("没有匹配标签 {q:?} 的版本"),
        ));
    }

    // 4. 包含匹配（模糊）
    if let Some(v) = versions.iter().find(|v| v.version.contains(q)) {
        return Ok(&v.version);
    }

    Err(EnvHiveError::new(
        EnvHiveErrorKind::VersionResolve,
        format!("无法从候选版本中解析 {q:?}（支持精确 / 前缀 / latest / lts / stable）"),
    ))
}

/// 在候选版本中解析 query：精确 → 前缀（取最高）→ 标签 → 包含匹配
/// `versions` 可为空（错误信息区分）。
/// 注意：仅含版本字符串、无 lts/labels 元数据，标签解析只能基于字符串启发式（见 matches_tag）。
pub fn resolve<'a>(versions: &'a [String], query: &str) -> Result<&'a str> {
    if versions.is_empty() {
        return Err(EnvHiveError::new(EnvHiveErrorKind::VersionNotFound, "可用版本列表为空"));
    }
    let q = query.trim().trim_start_matches('v');

    // 1. 精确匹配
    if let Some(v) = versions.iter().find(|v| v.as_str() == q) {
        return Ok(v);
    }

    // 2. 前缀匹配（收集所有命中，取最高版本）
    let mut hits: Vec<&String> = versions.iter().filter(|v| is_prefix_match(q, v)).collect();
    if !hits.is_empty() {
        hits.sort_by(|a, b| compare_versions(a, b));
        return Ok(hits.last().unwrap());
    }

    // 3. 标签
    if let Some(tag) = ResolveTag::from_str(q) {
        let tag_versions: Vec<&String> = versions
            .iter()
            .filter(|v| matches_tag(v, tag))
            .collect();
        if !tag_versions.is_empty() {
            let mut sorted = tag_versions.clone();
            sorted.sort_by(|a, b| compare_versions(a, b));
            return Ok(sorted.last().unwrap());
        }
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::VersionResolve,
            format!("没有匹配标签 {q:?} 的版本"),
        ));
    }

    // 4. 包含匹配（模糊）
    if let Some(v) = versions.iter().find(|v| v.contains(q)) {
        return Ok(v);
    }

    Err(EnvHiveError::new(
        EnvHiveErrorKind::VersionResolve,
        format!("无法从候选版本中解析 {q:?}（支持精确 / 前缀 / latest / lts / stable）"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::runtime::AvailableVersion;

    #[test]
    fn test_resolve_lts_meta() {
        let vers = vec![
            AvailableVersion { version: "21.0.10+7".into(), lts: true, labels: vec!["lts".into()] },
            AvailableVersion { version: "22.0.1+1".into(), lts: false, labels: vec![] },
            AvailableVersion { version: "17.0.9+9".into(), lts: true, labels: vec!["lts".into()] },
        ];
        // lts 应解析到最高的 LTS 版本（21），而非依赖版本字符串里出现 "lts" 字样
        assert_eq!(resolve_versions_meta(&vers, "lts").unwrap(), "21.0.10+7");
        // latest 取最高版本（22）
        assert_eq!(resolve_versions_meta(&vers, "latest").unwrap(), "22.0.1+1");
        // 精确 / 前缀仍可用
        assert_eq!(resolve_versions_meta(&vers, "17").unwrap(), "17.0.9+9");
    }
}

/// 是否匹配标签（基于真实元数据 lts / labels）。
/// stable：标签含 "stable" 或版本字符串无 pre-release（无 '-'）；
/// lts：v.lts 为真或标签含 "lts"；
/// latest：全放行（调用处取最高版本）。
fn matches_meta(v: &AvailableVersion, tag: ResolveTag) -> bool {
    match tag {
        ResolveTag::Latest => true,
        ResolveTag::Lts => v.lts || v.labels.iter().any(|l| l.eq_ignore_ascii_case("lts")),
        ResolveTag::Stable => {
            v.labels.iter().any(|l| l.eq_ignore_ascii_case("stable")) || !v.version.contains('-')
        }
    }
}

/// 是否匹配标签（仅字符串，无元数据时的回退）。stable = 不带 pre-release（无 '-'）。
fn matches_tag(v: &str, tag: ResolveTag) -> bool {
    let lower = v.to_ascii_lowercase();
    match tag {
        ResolveTag::Latest => true, // latest 在调用处取最高版本，这里全放行
        ResolveTag::Lts => lower.contains("lts"),
        ResolveTag::Stable => !v.contains('-'),
    }
}

/// 取候选列表中的最新版本（最高版本号）
pub fn latest<'a>(versions: &'a [String]) -> Option<&'a str> {
    let mut sorted: Vec<&String> = versions.iter().collect();
    sorted.sort_by(|a, b| compare_versions(a, b));
    sorted.last().map(|v| v.as_str())
}

/// 返回排序后的版本列表（升序；与外部顺序无关，稳定排序）
pub fn sorted(versions: &[String]) -> Vec<String> {
    let mut v = versions.to_vec();
    sort_versions(&mut v);
    v
}
