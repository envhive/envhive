//! EnvHiveManager · 下载加速镜像

use std::collections::HashMap;

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};

use super::EnvHiveManager;

impl EnvHiveManager {
    /// 下载加速镜像配置（读）
    pub fn download_mirror_config(&self) -> envhive_core::config::DownloadMirrorConfig {
        self.config.lock().unwrap().download_mirror.clone()
    }

    /// 生效的下载加速镜像配置（供版本列表 / 下载包 / checksum 全链路使用）。
    ///
    /// 语义（与 toolkit::mirror::apply 对齐）：
    /// - 规则仅取 `config.yaml downloadMirror.rules`（用户自定义 + 「加速镜像」下拉选中后
    ///   由 `set_tool_mirror` 写入），**不再无条件并入插件声明的全部候选** —— 否则未选中的
    ///   工具（官方源）会被首候选自动镜像，与 UI“官方源 = 不镜像”矛盾。
    /// - 已声明 `TOOL.mirrors` 的工具：把 `enable` 视为关闭（禁用内置兜底规则），镜像与否
    ///   完全由规则表决定：选了镜像 → 有规则即替换；官方源 → 无规则即走官方。
    /// - 未声明 mirrors 的旧工具：仍遵循总开关 + 内置兜底规则（BUILTIN_RULES）。
    pub fn effective_mirror(&self, desc: &dyn envhive_toolkit::tool::ToolDescriptor) -> envhive_core::config::DownloadMirrorConfig {
        let mut cfg = self.config.lock().unwrap().download_mirror.clone();
        if !desc.mirrors().is_empty() {
            // 已声明镜像候选的工具由显式规则接管，绕开内置兜底，避免“官方源”被劫持
            cfg.enable = false;
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
