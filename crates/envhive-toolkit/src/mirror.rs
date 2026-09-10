//! 下载加速镜像（roadmap P2：蜂巢自身 工具 下载的国内加速源）
//!
//! 原理：对官方下载 URL 做前缀替换。规则优先级：
//! **显式规则**（config.yaml `downloadMirror.rules`：用户自定义 + 「加速镜像」下拉选中
//! 后写入，见 manager::set_tool_mirror）> **内置兜底规则**（BUILTIN_RULES，仅对未声明
//! mirrors 的旧 工具 生效，由 `enable` 总开关控制）。
//!
//! 语义约定：显式规则**选即生效、不受 enable 门控**（下拉选择镜像即自足，无需另开总开关）；
//! `enable` 只控制内置兜底规则（旧工具的默认加速）。manager::effective_mirror 会对
//! 已声明 `TOOL.mirrors` 的 工具 关闭 enable，避免“官方源”被内置兜底劫持。
//!
//! 该模块同时作用于：版本列表 URL（node index.json / rust channel toml）、
//! 下载包 URL、checksum URL（保证与下载同源）。

use envhive_core::config::DownloadMirrorConfig;

/// 内置兜底规则（按顺序匹配，前缀命中即替换）。
/// 面向未声明 `TOOL.mirrors` 的旧 工具；声明了 mirrors 的 工具 请改用插件候选 + 下拉切换。
pub const BUILTIN_RULES: &[(&str, &str)] = &[
    ("https://nodejs.org/dist/", "https://npmmirror.com/mirrors/node/"),
    ("https://dl.google.com/go/", "https://mirrors.aliyun.com/golang/"),
    ("https://static.rust-lang.org/dist/", "https://rsproxy.cn/dist/"),
];

/// 对 URL 应用镜像替换。
/// 1. 显式规则（`cfg.rules`：已选镜像 / 用户自定义）命中即替换，**不依赖 enable**；
/// 2. 内置兜底规则仅在 `enable = true` 时生效（旧 工具 默认加速）。
/// `cfg` 为 None 视为未配置，返回原 URL。
pub fn apply(cfg: Option<&DownloadMirrorConfig>, url: &str) -> String {
    let Some(cfg) = cfg else { return url.to_string() };
    // 显式规则：下拉选中镜像即写入此处，选即生效（无需 enable 总开关）
    for (k, v) in &cfg.rules {
        if url.starts_with(k.as_str()) {
            let out = url.replacen(k, v, 1);
            tracing::debug!("镜像替换: {url} -> {out}");
            return out;
        }
    }
    // 内置兜底：仅总开关开启（未声明 mirrors 的旧 工具）
    if cfg.enable {
        for (k, v) in BUILTIN_RULES {
            if url.starts_with(k) {
                let out = url.replacen(k, v, 1);
                tracing::debug!("镜像替换: {url} -> {out}");
                return out;
            }
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
    fn test_explicit_rule_applies_without_enable() {
        // 核心修复：下拉选中的镜像写入 rules 后，即使 enable=false 也立即生效（无需总开关）
        let mut c = cfg(false);
        c.rules.insert("https://nodejs.org/dist/".into(), "https://mirrors.huaweicloud.com/nodejs/".into());
        assert_eq!(
            apply(Some(&c), "https://nodejs.org/dist/v22.0.0/node-v22-win-x64.zip"),
            "https://mirrors.huaweicloud.com/nodejs/v22.0.0/node-v22-win-x64.zip"
        );
    }

    #[test]
    fn test_node_mirror() {
        // enable=true 仅放行内置兜底（面向未声明 mirrors 的旧工具）
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
