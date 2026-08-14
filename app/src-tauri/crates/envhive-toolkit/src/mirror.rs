//! 下载加速镜像（roadmap P2：蜂巢自身 工具 下载的国内加速源）
//!
//! 原理：对官方下载 URL 做前缀替换。规则优先级：**用户自定义规则**（config.yaml，
//! 设置页可增删）> **插件声明的镜像候选**（TOOL.mirrors，各 工具 可切换多个地址）>
//! **内置兜底规则**（下方 BUILTIN_RULES，仅对未声明 mirrors 的旧 工具 生效）。
//!
//! 该模块同时作用于：版本列表 URL（node index.json / rust channel toml）、
//! 下载包 URL、checksum URL（保证与下载同源）。

use envhive_core::config::DownloadMirrorConfig;

/// 内置兜底规则（按顺序匹配，前缀命中即替换）。
/// 新 工具 请优先在插件里声明 `TOOL.mirrors`，宿主会将其聚合进规则表（见 manager::effective_mirror）。
pub const BUILTIN_RULES: &[(&str, &str)] = &[
    ("https://nodejs.org/dist/", "https://npmmirror.com/mirrors/node/"),
    ("https://dl.google.com/go/", "https://mirrors.aliyun.com/golang/"),
    ("https://static.rust-lang.org/dist/", "https://rsproxy.cn/dist/"),
];

/// 对 URL 应用镜像替换；未开启镜像或无可匹配规则时返回原 URL。
/// `cfg` 为 None 视为未开启。
pub fn apply(cfg: Option<&DownloadMirrorConfig>, url: &str) -> String {
    let Some(cfg) = cfg else { return url.to_string() };
    if !cfg.enable {
        return url.to_string();
    }
    // 用户自定义规则优先
    for (k, v) in &cfg.rules {
        if url.starts_with(k.as_str()) {
            let out = url.replacen(k, v, 1);
            tracing::debug!("镜像替换: {url} -> {out}");
            return out;
        }
    }
    for (k, v) in BUILTIN_RULES {
        if url.starts_with(k) {
            let out = url.replacen(k, v, 1);
            tracing::debug!("镜像替换: {url} -> {out}");
            return out;
        }
    }
    url.to_string()
}

/// 已镜像的 URL（用于日志展示）
pub fn display(url: &str, cfg: Option<&DownloadMirrorConfig>) -> String {
    let mirrored = apply(cfg, url);
    if mirrored == url { url.to_string() } else { format!("{url}（镜像 → {mirrored}）") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn cfg(enable: bool) -> DownloadMirrorConfig {
        DownloadMirrorConfig { enable, rules: HashMap::new() }
    }

    #[test]
    fn test_disabled() {
        let c = cfg(false);
        assert_eq!(apply(Some(&c), "https://nodejs.org/dist/v22.0.0/node-v22-win-x64.zip"), "https://nodejs.org/dist/v22.0.0/node-v22-win-x64.zip");
    }

    #[test]
    fn test_node_mirror() {
        let c = cfg(true);
        assert_eq!(
            apply(Some(&c), "https://nodejs.org/dist/v22.0.0/node-v22-win-x64.zip"),
            "https://npmmirror.com/mirrors/node/v22.0.0/node-v22-win-x64.zip"
        );
    }

    #[test]
    fn test_go_mirror() {
        let c = cfg(true);
        assert_eq!(
            apply(Some(&c), "https://dl.google.com/go/go1.24.1.windows-amd64.zip.sha256"),
            "https://mirrors.aliyun.com/golang/go1.24.1.windows-amd64.zip.sha256"
        );
    }

    #[test]
    fn test_rust_mirror() {
        let c = cfg(true);
        assert_eq!(
            apply(Some(&c), "https://static.rust-lang.org/dist/channel-rust-stable.toml"),
            "https://rsproxy.cn/dist/channel-rust-stable.toml"
        );
    }

    #[test]
    fn test_user_rule_overrides() {
        let mut c = cfg(true);
        c.rules.insert("https://nodejs.org/dist/".into(), "https://custom.example/node/".into());
        assert_eq!(
            apply(Some(&c), "https://nodejs.org/dist/index.json"),
            "https://custom.example/node/index.json"
        );
    }

    #[test]
    fn test_no_match() {
        let c = cfg(true);
        assert_eq!(apply(Some(&c), "https://api.adoptium.net/v3/foo"), "https://api.adoptium.net/v3/foo");
    }
}
