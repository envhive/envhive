//! 使用统计（roadmap P2：各项目、各 工具 版本的使用频率，辅助释放磁盘）
//!
//! 存储：`~/.envhive/state/usage.json`
//! ```json
//! { "versions": { "nodejs@22.11.0": { "count": 5, "first_used_at": ..., "last_used_at": ..., "project_hits": { "myapp": 3 } } } }
//! ```
//! 记录时机：安装成功、全局切换成功。磁盘占用按需扫描（统计面板打开时）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use envhive_core::error::{EnvHiveError, Result};
use crate::manager::EnvHiveManager;
use envhive_core::pathmeta::PathMeta;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageStore {
    /// key = "tool@version"
    pub versions: HashMap<String, UsageRecord>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageRecord {
    pub count: u32,
    pub first_used_at: i64,
    pub last_used_at: i64,
    pub project_hits: HashMap<String, u32>,
}

fn usage_file(paths: &PathMeta) -> PathBuf {
    paths.state.join("usage.json")
}

fn load(paths: &PathMeta) -> UsageStore {
    std::fs::read_to_string(usage_file(paths))
        .ok()
        .and_then(|s| serde_json::from_str::<UsageStore>(&s).ok())
        .unwrap_or_default()
}

fn save(paths: &PathMeta, store: &UsageStore) -> Result<()> {
    let file = usage_file(paths);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&file, serde_json::to_string_pretty(store).map_err(EnvHiveError::from)?)?;
    Ok(())
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 记录一次使用（安装 / 切换成功时调用）
pub fn record_use(paths: &PathMeta, tool: &str, version: &str, project: Option<&str>) {
    if tool.is_empty() || version.is_empty() {
        return;
    }
    let mut store = load(paths);
    let key = format!("{tool}@{version}");
    let t = now();
    let rec = store.versions.entry(key).or_insert_with(|| UsageRecord {
        count: 0,
        first_used_at: t,
        last_used_at: 0,
        project_hits: HashMap::new(),
    });
    rec.count = rec.count.saturating_add(1);
    rec.last_used_at = t;
    if let Some(p) = project {
        let hits = rec.project_hits.entry(p.to_string()).or_insert(0);
        *hits = hits.saturating_add(1);
    }
    if let Err(e) = save(paths, &store) {
        tracing::warn!("写入使用统计失败: {e}");
    }
}

// ---------------------------------------------------------------------------
// 统计聚合（磁盘占用按需扫描）
// ---------------------------------------------------------------------------

/// 统计面板输出（聚合）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageStats {
    /// 记录过的 工具@version 总数
    pub total_versions: usize,
    /// 全部已安装版本的磁盘占用（字节）
    pub total_disk_bytes: u64,
    /// 按 工具 分组
    pub tool_usage: Vec<ToolUsage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUsage {
    pub tool: String,
    pub display: String,
    pub total_count: u32,
    pub disk_bytes: u64,
    pub versions: Vec<VersionUsage>,
    /// 工具 图标 data URI（无图标为 None，前端回退彩色圆点）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionUsage {
    pub version: String,
    pub count: u32,
    pub first_used_at: Option<i64>,
    pub last_used_at: Option<i64>,
    /// 距上次使用的天数（未使用过为 None）
    pub last_used_days_ago: Option<i64>,
    pub disk_bytes: u64,
    pub installed: bool,
    pub is_current: bool,
    pub projects: Vec<ProjectHit>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHit {
    pub project: String,
    pub hits: u32,
}

/// 聚合使用统计：内置 + 插件 工具，含磁盘占用扫描
pub fn stats(manager: &EnvHiveManager) -> Result<UsageStats> {
    let store = load(&manager.paths);
    let chain = manager.chain(None)?;
    let mut tool_usage: Vec<ToolUsage> = Vec::new();
    let mut total_disk = 0u64;

    for name in manager.all_sdk_names() {
        let Ok(tool) = manager.lookup_tool(&name) else { continue };
        let display = tool.desc().display().to_string();
        // 图标：插件目录文件（svg>png>jpg）优先，其次插件声明 base64
        let icon = envhive_toolkit::tool::icon::resolve_icon(&manager.paths.plugins.join(&name), tool.desc().icon_base64());
        let installed = tool.installed_versions(&manager.paths);
        let current = chain.tool(&name).map(|t| t.version().to_string());
        let mut versions: Vec<VersionUsage> = Vec::new();
        let mut sdk_count = 0u32;
        let mut sdk_disk = 0u64;

        for v in installed {
            let ver = v.as_str().to_string();
            let key = format!("{name}@{ver}");
            let rec = store.versions.get(&key);
            let disk = dir_size(&manager.paths.version_dir(&name, &ver)).unwrap_or(0);
            let count = rec.map(|r| r.count).unwrap_or(0);
            sdk_count += count;
            sdk_disk += disk;
            total_disk += disk;
            let last_used_days_ago = rec
                .filter(|r| r.last_used_at > 0)
                .map(|r| (now() - r.last_used_at).max(0) / 86400);
            let mut projects: Vec<ProjectHit> = rec
                .map(|r| {
                    r.project_hits
                        .iter()
                        .map(|(p, h)| ProjectHit { project: p.clone(), hits: *h })
                        .collect()
                })
                .unwrap_or_default();
            projects.sort_by(|a, b| b.hits.cmp(&a.hits));
            versions.push(VersionUsage {
                version: ver.clone(),
                count,
                first_used_at: rec.map(|r| r.first_used_at),
                last_used_at: rec.map(|r| r.last_used_at),
                last_used_days_ago,
                disk_bytes: disk,
                installed: true,
                is_current: current.as_deref() == Some(ver.as_str()),
                projects,
            });
        }
        versions.sort_by(|a, b| b.last_used_at.unwrap_or(0).cmp(&a.last_used_at.unwrap_or(0)));
        tool_usage.push(ToolUsage { tool: name, display, total_count: sdk_count, disk_bytes: sdk_disk, versions, icon });
    }

    tool_usage.sort_by(|a, b| b.total_count.cmp(&a.total_count));
    Ok(UsageStats {
        total_versions: store.versions.len(),
        total_disk_bytes: total_disk,
        tool_usage,
    })
}

/// 递归统计目录大小
fn dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_dir() {
            total += dir_size(&entry.path())?;
        } else {
            total += entry.metadata()?.len();
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use envhive_core::pathmeta;

    #[test]
    fn test_record_and_stats_path() {
        let root = std::env::temp_dir().join(format!("envhive-usage-test-{}", std::process::id()));
        let paths = pathmeta::from_root(&root);
        std::fs::create_dir_all(&paths.state).unwrap();
        record_use(&paths, "nodejs", "22.11.0", Some("myapp"));
        record_use(&paths, "nodejs", "22.11.0", Some("myapp"));
        record_use(&paths, "nodejs", "20.9.0", None);

        let store = load(&paths);
        assert_eq!(store.versions.len(), 2);
        assert_eq!(store.versions["nodejs@22.11.0"].count, 2);
        assert_eq!(store.versions["nodejs@22.11.0"].project_hits["myapp"], 2);
        let _ = std::fs::remove_dir_all(&root);
    }
}
