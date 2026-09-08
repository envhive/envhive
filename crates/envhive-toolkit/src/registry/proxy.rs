//! proxy 统一管理（roadmap H：两层联动）
//! 1. 蜂巢下载代理：config.proxy → reqwest Client（Manager 持有，动态重建）+ Lua 插件 http.get
//! 2. 工具代理：向 npm / pip 配置写入代理键
//! 生效范围仅限蜂巢应用内（工具 下载 / 版本列表 / 校验与插件请求），不写入系统全局环境变量。
//! 历史版本（旧第 3 层）曾写用户注册表 HTTP_PROXY/HTTPS_PROXY/NO_PROXY，由 cleanup_legacy_env_vars 清理。

use envhive_core::config::ProxyConfig;
use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};

/// 校验代理地址格式
pub fn validate_proxy_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with("socks5://")
}

/// 构建 reqwest 代理（蜂巢下载代理，第 1 层）
pub fn build_reqwest_proxy(cfg: &ProxyConfig) -> Result<Option<reqwest::Proxy>> {
    if !cfg.enable {
        return Ok(None);
    }
    let Some(url) = &cfg.url else { return Ok(None) };
    if !validate_proxy_url(url) {
        return Err(EnvHiveError::new(EnvHiveErrorKind::Config, format!("代理地址无效: {url}")));
    }
    Ok(Some(reqwest::Proxy::all(url).map_err(|e| {
        EnvHiveError::with_source(EnvHiveErrorKind::Config, format!("代理地址解析失败: {url}"), e)
    })?))
}

/// 工具代理注入（第 2 层）：向 npm/pip 配置写入代理键
/// 返回受影响工具列表
pub fn inject_tool_proxy(proxy: &ProxyConfig) -> Result<Vec<String>> {
    let mut affected = Vec::new();

    // npm：proxy= / https-proxy= / strict-ssl=
    if let Some(url) = &proxy.url {
        let mut updates = vec![("proxy", url.as_str()), ("https-proxy", url.as_str())];
        if proxy.enable {
            updates.push(("strict-ssl", "false"));
        } else {
            updates.push(("strict-ssl", "true"));
        }
        let npm = npm::write(&updates, !proxy.enable)?;
        if npm {
            affected.push("npm".to_string());
        }

        // pip：[global] proxy=
        let pip = pip::write(&[("proxy", url.as_str())], !proxy.enable)?;
        if pip {
            affected.push("pip".to_string());
        }
    } else if !proxy.enable {
        // 关闭代理：移除工具代理键
        let npm = npm::write(&[], true)?;
        if npm {
            affected.push("npm".to_string());
        }
        let pip = pip::write(&[], true)?;
        if pip {
            affected.push("pip".to_string());
        }
    }
    Ok(affected)
}

mod npm {
    use std::path::PathBuf;

    use super::*;

    pub fn write(updates: &[(&str, &str)], clear: bool) -> Result<bool> {
        let path = npmrc_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let mut merged = crate::registry::npm::merge_ini(&content, updates, if clear { &["proxy", "https-proxy"] } else { &[] });
        // 清理残留（unset 场景）
        if clear {
            merged = crate::registry::npm::merge_ini(&merged, updates, &["proxy", "https-proxy", "strict-ssl"]);
        }
        if merged == content && !path.exists() {
            return Ok(false);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, merged)?;
        Ok(true)
    }

    fn npmrc_path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Io, "无法确定主目录"))?;
        Ok(home.join(".npmrc"))
    }
}

mod pip {
    use std::path::PathBuf;

    use super::*;

    pub fn write(updates: &[(&str, &str)], clear: bool) -> Result<bool> {
        let path = pip_path()?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let merged = crate::registry::npm::merge_ini_section(&content, "global", updates, if clear { &["proxy"] } else { &[] });
        if merged == content && !path.exists() {
            return Ok(false);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, merged)?;
        Ok(true)
    }

    fn pip_path() -> Result<PathBuf> {
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

/// 清理历史版本（旧第 3 层）写入用户环境变量（注册表）的代理残留。
/// 仅当注册表 HTTP_PROXY 现值等于蜂巢配置 url（开启时写入）或空串（关闭时写入的残留）
/// 才删除三个键，避免误删用户手动配置；旧版三个键总是同时写入，以 HTTP_PROXY 为判断基准。
/// 应用内生效策略下不再注入环境变量，此函数仅用于升级后的一次性清理。
#[cfg(windows)]
pub fn cleanup_legacy_env_vars(cfg: &ProxyConfig) -> Result<()> {
    use envhive_core::env::registry_windows as reg;
    let Some(prev_url) = &cfg.url else { return Ok(()) };
    let current = match reg::get_user_env_var("HTTP_PROXY") {
        Ok(v) => v,
        Err(_) => None,
    };
    let is_legacy = match &current {
        Some(v) => v == prev_url || v.is_empty(),
        None => false,
    };
    if !is_legacy {
        return Ok(());
    }
    let _ = reg::delete_user_env_var("HTTP_PROXY");
    let _ = reg::delete_user_env_var("HTTPS_PROXY");
    let _ = reg::delete_user_env_var("NO_PROXY");
    reg::broadcast_env_change();
    tracing::info!("已清理蜂巢历史写入的代理环境变量（旧第 3 层残留）");
    Ok(())
}

#[cfg(not(windows))]
pub fn cleanup_legacy_env_vars(_cfg: &ProxyConfig) -> Result<()> {
    Ok(())
}

/// 可用性探测（roadmap H：代理不通提示回退）——对目标源 HEAD 请求
pub async fn probe(url: &str, proxy: Option<&ProxyConfig>) -> bool {
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("envhive/0.1.0");
    if let Some(p) = proxy {
        if let Ok(px) = build_reqwest_proxy(p) {
            if let Some(px) = px {
                builder = builder.proxy(px);
            }
        }
    }
    let Ok(client) = builder.build() else { return false };
    match client.head(url).send().await {
        Ok(r) => r.status().is_success() || r.status().is_redirection(),
        Err(_) => false,
    }
}
