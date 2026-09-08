//! EnvHiveManager · 下载加速镜像

use std::collections::HashMap;

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};

use super::EnvHiveManager;

impl EnvHiveManager {
    /// 下载加速镜像配置（读）
    pub fn download_mirror_config(&self) -> envhive_core::config::DownloadMirrorConfig {
        self.config.lock().unwrap().download_mirror.clone()
    }

    /// 生效的下载加速镜像配置：用户自定义规则（config.yaml，优先）+
    /// 该 工具 插件声明的镜像候选（TOOL.mirrors，缺省为不镜像时无规则）。
    /// 规则语义：from 前缀 → to 新前缀，作用于版本列表 / 下载包 / checksum 全链路。
    pub fn effective_mirror(&self, desc: &dyn envhive_toolkit::tool::ToolDescriptor) -> envhive_core::config::DownloadMirrorConfig {
        let mut cfg = self.config.lock().unwrap().download_mirror.clone();
        for m in desc.mirrors() {
            cfg.rules.entry(m.from.clone()).or_insert_with(|| m.to.clone());
        }
        cfg
    }

    /// 生效镜像规则列表（from→to，供插件 http.get 注入）
    pub fn effective_mirror_rules(&self, desc: &dyn envhive_toolkit::tool::ToolDescriptor) -> Vec<(String, String)> {
        self.effective_mirror(desc)
            .rules
            .into_iter()
            .collect()
    }

    /// 设置某 工具 的下载加速镜像（按插件声明切换多地址）：
    /// `mirror_name` = None → 官方源（移除该 工具 的前缀替换规则）；
    /// Some(name) → 在插件声明的 mirrors 中查找并启用。
    /// 返回更新后的完整镜像配置（前端可直接覆盖 state）。
    pub fn set_tool_mirror(
        &self,
        tool: &str,
        mirror_name: Option<&str>,
    ) -> Result<envhive_core::config::DownloadMirrorConfig> {
        let sdk_arc = self.lookup_tool(tool).map_err(|e| {
            EnvHiveError::with_source(
                EnvHiveErrorKind::ToolNotFound,
                format!("设置镜像失败：工具 {tool} 不存在"),
                e,
            )
        })?;
        let mirrors = sdk_arc.desc().mirrors().to_vec();
        if mirrors.is_empty() {
            return Err(EnvHiveError::new(
                EnvHiveErrorKind::Config,
                format!("工具 {tool} 未声明下载加速镜像（插件 TOOL.mirrors 缺失）"),
            ));
        }
        {
            let mut cfg = self.config.lock().unwrap();
            // 先移除该 工具 声明过的所有规则（官方 = 不镜像）
            let froms: Vec<String> = mirrors.iter().map(|m| m.from.clone()).collect();
            for f in &froms {
                cfg.download_mirror.rules.remove(f);
            }
            if let Some(name) = mirror_name {
                if name.is_empty() || name == "官方" {
                    // 官方源：规则已移除
                } else {
                    // 同名多 from（如 maven 同时覆盖 dlcdn 与 archive）全部启用：
                    // 仅插入第一条会在 archive URL 命中时被忽略，导致旧版本仍走官方源
                    let matched: Vec<_> = mirrors.iter().filter(|m| m.name == name).collect();
                    if matched.is_empty() {
                        return Err(EnvHiveError::new(
                            EnvHiveErrorKind::Config,
                            format!("工具 {tool} 未声明镜像「{name}」（可选：{}）", mirrors.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(" / ")),
                        ));
                    }
                    for cand in matched {
                        cfg.download_mirror.rules.insert(cand.from.clone(), cand.to.clone());
                    }
                }
            }
            let out = cfg.download_mirror.clone();
            cfg.save(&self.paths.config_file)?;
            tracing::info!("[{tool}] 下载加速镜像: {:?}", mirror_name.map(str::to_string));
            Ok(out)
        }
    }

    /// 设置下载加速镜像（写 config.yaml；对后续下载 / 版本列表生效）
    pub fn set_download_mirror(
        &self,
        enable: bool,
        rules: Option<HashMap<String, String>>,
    ) -> Result<()> {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.download_mirror.enable = enable;
            if let Some(r) = rules {
                cfg.download_mirror.rules = r;
            }
            cfg.save(&self.paths.config_file)?;
        }
        tracing::info!("下载加速镜像: {}", if enable { "开启" } else { "关闭" });
        Ok(())
    }
}
