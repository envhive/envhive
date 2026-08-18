//! EnvHiveManager · 镜像源（registry）与代理

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};

use super::EnvHiveManager;

impl EnvHiveManager {
    pub fn apply_registry(&self, tool: &str, preset: &str) -> Result<()> {
        let customs = self.config.lock().unwrap().custom_registry.clone();
        self.registry.apply(tool, preset, &customs)
    }

    pub fn registry_state(&self, tool: &str) -> Result<envhive_toolkit::registry::RegistryState> {
        let mut st = self.registry.state(tool)?;
        // 内置未识别 → 再匹配用户自定义源（写入后状态高亮「自定义」）
        if st.preset_name.is_none() {
            if let Some(u) = &st.current_url {
                let customs = self.config.lock().unwrap().custom_registry.clone();
                if let Some(c) = customs
                    .iter()
                    .find(|c| c.tool == tool && (u.contains(&c.url) || c.url.contains(u)))
                {
                    st.preset_name = Some(c.name.clone());
                }
            }
        }
        Ok(st)
    }

    /// 添加用户自定义镜像源（config.yaml 持久化；同名覆盖）
    pub fn add_custom_registry_preset(&self, tool: &str, name: &str, url: &str) -> Result<()> {
        envhive_toolkit::registry::ensure_name("预设", name)?;
        let name = name.trim().to_string();
        let url = url.trim().trim_end_matches('/').to_string();
        if name.is_empty() {
            return Err(EnvHiveError::new(EnvHiveErrorKind::Config, "自定义源名称不能为空"));
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(EnvHiveError::new(EnvHiveErrorKind::Config, "镜像地址需以 http:// 或 https:// 开头"));
        }
        // 与内置预设重名冲突校验
        if envhive_toolkit::registry::find_preset(tool, &name).is_some() {
            return Err(EnvHiveError::new(
                EnvHiveErrorKind::Config,
                format!("{tool} 已存在内置预设 {name:?}，请换一个名称"),
            ));
        }
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.custom_registry.retain(|c| !(c.tool == tool && c.name == name));
            cfg.custom_registry.push(envhive_core::config::CustomRegistryPreset {
                tool: tool.to_string(),
                name: name.clone(),
                url: url.clone(),
            });
            cfg.save(&self.paths.config_file)?;
        }
        tracing::info!("[{tool}] 已添加自定义镜像源 {name} -> {url}");
        Ok(())
    }

    /// 删除用户自定义镜像源
    pub fn remove_custom_registry_preset(&self, tool: &str, name: &str) -> Result<()> {
        let mut cfg = self.config.lock().unwrap();
        let before = cfg.custom_registry.len();
        cfg.custom_registry.retain(|c| !(c.tool == tool && c.name == name));
        if cfg.custom_registry.len() == before {
            return Err(EnvHiveError::new(EnvHiveErrorKind::Config, format!("{tool} 无自定义源 {name:?}")));
        }
        cfg.save(&self.paths.config_file)?;
        tracing::info!("[{tool}] 已删除自定义镜像源 {name}");
        Ok(())
    }

    /// 设置代理（写 config.yaml + 重建下载 client + 工具代理；仅应用内生效，不写系统环境变量）
    pub fn set_proxy(&self, url: Option<String>, enable: bool) -> Result<()> {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.proxy.url = url;
            cfg.proxy.enable = enable;
            cfg.save(&self.paths.config_file)?;
        }
        // 重建下载 client（含插件 http.get 代理同步）+ 注入工具代理
        self.rebuild_client();
        let proxy = self.config.lock().unwrap().proxy.clone();
        envhive_toolkit::registry::proxy::inject_tool_proxy(&proxy)?;
        Ok(())
    }
}
