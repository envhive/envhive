//! 项目预设（UI v2 · 首页「项目环境」区）：项目 = 目录 + 命名 + 版本组合。
//! 持久化到 `~/.envhive/state/projects.json`（与指纹 / 使用统计同级的 state 目录）。
//! 语义：会话级注入 —— 「以此环境启动」通过 launch_session 注入该组合的
//! JAVA_HOME / PATH 等变量，不写系统变量，关窗即消失。

use serde::{Deserialize, Serialize};

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use envhive_core::pathmeta::PathMeta;

/// 版本组合中的一项：tool + 发行商（可选）+ 版本
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectToolVersion {
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distribution: Option<String>,
    pub version: String,
}

/// 项目预设：名称 + 目录 + 版本组合
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPreset {
    pub name: String,
    pub dir: String,
    /// 版本组合（顺序即注入优先级：后者覆盖前者同名变量）
    pub versions: Vec<ProjectToolVersion>,
}

fn file(paths: &PathMeta) -> std::path::PathBuf {
    paths.state.join("projects.json")
}

/// 读取全部项目预设（文件不存在 / 为空返回空列表）
pub fn load(paths: &PathMeta) -> Result<Vec<ProjectPreset>> {
    let p = file(paths);
    if !p.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&p)?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<ProjectPreset> = serde_json::from_str(&content).map_err(|e| {
        // 解析失败时同时落日志（含文件路径与 serde 细节），避免 log 无信息
        let msg = format!("解析项目预设文件失败（{}）: {e}", p.display());
        tracing::error!("{}", msg);
        EnvHiveError::with_source(EnvHiveErrorKind::Config, msg, e)
    })?;
    Ok(list)
}

/// 全量写回（原子：先写临时文件再改名）
pub fn save(paths: &PathMeta, presets: &[ProjectPreset]) -> Result<()> {
    let p = file(paths);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(presets).map_err(|e| {
        let msg = format!("序列化项目预设失败: {e}");
        tracing::error!("{}", msg);
        EnvHiveError::with_source(EnvHiveErrorKind::Config, msg, e)
    })?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}
