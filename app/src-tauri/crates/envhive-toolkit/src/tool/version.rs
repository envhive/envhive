//! 版本类型与排序（roadmap E：点分数字逐段比较，兼容 `1.9 < 1.10 < 2.0-rc1`、`2024.10.12`）

use std::cmp::Ordering;
use std::fmt;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct Version(pub String);

impl Version {
    pub fn new(s: impl Into<String>) -> Self {
        Version(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 版本目录名（v- 前缀）
    pub fn dir_name(&self) -> String {
        format!("v-{}", self.0)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(compare_versions(&self.0, &other.0))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_versions(&self.0, &other.0)
    }
}

/// 版本段：数字或字符串
#[derive(Debug, Clone, PartialEq, Eq)]
enum Seg {
    Num(u64),
    Str(String),
}

impl Ord for Seg {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Seg::Num(a), Seg::Num(b)) => a.cmp(b),
            // 数字段 < 字符串段（"1.2a" 视为略大于 "1.2" 的语义由调用方处理，这里稳定排序即可）
            (Seg::Num(_), Seg::Str(_)) => Ordering::Less,
            (Seg::Str(_), Seg::Num(_)) => Ordering::Greater,
            (Seg::Str(a), Seg::Str(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for Seg {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// 拆分版本为 (主干段, pre-release 部分, 是否 pre-release)
fn split_version(v: &str) -> (Vec<Seg>, Option<Vec<Seg>>, bool) {
    let mut s = v.trim().trim_start_matches('v').trim_start_matches('V').to_string();
    // 先丢弃 build 元数据（+build 不影响优先级，遵循 semver；Adoptium/Temurin 版本形如 "21.0.10+7"）。
    // 否则 "+7" 会被当成字符串段，导致 "21.0.10+7" < "21.0.9+9" 这种错误排序。
    if let Some(idx) = s.find('+') {
        s.truncate(idx);
    }
    let v = s.as_str();
    // pre-release：第一个 '-' 之后（如 "2.0-rc1"）
    let (core, pre) = match v.find('-') {
        Some(idx) => (&v[..idx], Some(&v[idx + 1..])),
        None => (v, None),
    };

    let segs = |s: &str| -> Vec<Seg> {
        s.split(['.', '_'])
            .filter(|x| !x.is_empty())
            .map(|x| match x.parse::<u64>() {
                Ok(n) => Seg::Num(n),
                Err(_) => Seg::Str(x.to_string()),
            })
            .collect()
    };

    let core_segs = segs(core);
    let pre_segs = pre.map(segs);
    let is_pre = pre.is_some();
    (core_segs, pre_segs, is_pre)
}

/// 版本比较：点分数字逐段比较；pre-release < release；主干相同则 pre 逐段比较。
/// `1.9 < 1.10 < 2.0-rc1 < 2.0`；`21.5.1 > 21.5`（多段者大）。
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let (a_core, a_pre, a_is_pre) = split_version(a);
    let (b_core, b_pre, b_is_pre) = split_version(b);

    // 主干逐段比较
    let len = a_core.len().max(b_core.len());
    for i in 0..len {
        let sa = a_core.get(i);
        let sb = b_core.get(i);
        let ord = match (sa, sb) {
            (Some(x), Some(y)) => x.cmp(y),
            // 一段为空：数字段优先（多段者大，如 21.5.1 > 21.5）
            (Some(Seg::Num(_)), None) => Ordering::Greater,
            (None, Some(Seg::Num(_))) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            (None, None) => Ordering::Equal,
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }

    // pre-release 比较：release > pre-release
    match (a_is_pre, b_is_pre) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }

    // 都带 pre：逐段比较
    match (&a_pre, &b_pre) {
        (Some(x), Some(y)) => {
            let len = x.len().max(y.len());
            for i in 0..len {
                let ord = match (x.get(i), y.get(i)) {
                    (Some(sa), Some(sb)) => sa.cmp(sb),
                    (Some(_), None) => Ordering::Greater,
                    (None, Some(_)) => Ordering::Less,
                    (None, None) => Ordering::Equal,
                };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            Ordering::Equal
        }
        _ => Ordering::Equal,
    }
}

/// 版本列表排序（升序）
pub fn sort_versions(versions: &mut [String]) {
    versions.sort_by(|a, b| compare_versions(a, b));
}

/// 判断 version 是否为 query 的前缀匹配（"21" → "21.5.1"）
pub fn is_prefix_match(query: &str, version: &str) -> bool {
    let q = query.trim_start_matches('v');
    let v = version.trim_start_matches('v');
    if q.is_empty() {
        return false;
    }
    if v == q {
        return true;
    }
    // "21" 匹配 "21." 开头，或完全等于（精确已在上层处理）
    v.starts_with(q) && (v.as_bytes().get(q.len()) == Some(&b'.') || v.as_bytes().get(q.len()) == Some(&b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare() {
        assert!(compare_versions("1.9", "1.10") == Ordering::Less);
        assert!(compare_versions("1.10", "2.0") == Ordering::Less);
        assert!(compare_versions("2.0-rc1", "2.0") == Ordering::Less);
        assert!(compare_versions("2.0-rc1", "2.0-rc2") == Ordering::Less);
        assert!(compare_versions("21.5.1", "21.5") == Ordering::Greater);
        assert!(compare_versions("2024.10.12", "2024.2.1") == Ordering::Greater);
        assert!(compare_versions("22.11.0", "22.11.0") == Ordering::Equal);
        assert!(compare_versions("1.84.0", "1.84.1") == Ordering::Less);
    }

    #[test]
    fn test_prefix() {
        assert!(is_prefix_match("21", "21.5.1"));
        assert!(is_prefix_match("21", "21"));
        assert!(!is_prefix_match("21", "2.1.5"));
        assert!(is_prefix_match("1.2", "1.2.3"));
    }

    #[test]
    fn test_build_metadata_ignored() {
        // +build 不影响优先级（semver）：21.0.10+7 应高于 21.0.9+9
        // 旧实现把 "+7" 当字符串段，导致 21.0.10+7 < 21.0.9+9（Adoptium/Temurin 排序全错）
        assert!(compare_versions("21.0.10+7", "21.0.9+9") == Ordering::Greater);
        assert!(compare_versions("21.0.9+9", "21.0.10+7") == Ordering::Less);
        assert!(compare_versions("21.0.10+7", "21.0.10+7") == Ordering::Equal);
        assert!(compare_versions("11.0.2+9", "11.0.25+9") == Ordering::Less);
    }
}
