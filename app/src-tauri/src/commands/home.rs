//! 首页总览：当前系统使用的 工具 及其注入的环境变量

use serde::Serialize;
use tauri::State;

use crate::error::Result;

use super::AppState;

/// 单个环境变量键值
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvKeyValue {
    pub key: String,
    pub value: String,
}

/// 单个 工具 注入明细
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolEnvInfo {
    pub name: String,
    pub display: String,
    pub category: String,
    /// 当前链上使用的版本（未配置则为 None）
    pub version: Option<String>,
    /// 是否当前生效（已安装 + 链接存在）
    pub active: bool,
    /// 注入的环境变量（如 JAVA_HOME；不含 PATH）
    pub vars: Vec<EnvKeyValue>,
    /// 注入的 PATH 前缀（按优先级从高到低）
    pub path_entries: Vec<String>,
    /// 工具 图标 data URI（无图标为 None，前端回退彩色圆点）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// 合并后的环境变量行（含来源，供首页总表）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvVarRow {
    pub key: String,
    pub value: String,
    /// 来源：工具 展示名 / "全局配置"
    pub source: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeOverview {
    /// 全部 工具（含未使用的）及其注入明细
    pub tools: Vec<ToolEnvInfo>,
    /// 合并后的完整环境变量（PATH 逐条列出；按 key 排序）
    pub merged: Vec<EnvVarRow>,
}

/// 首页：当前系统使用的 工具 注入的环境变量（与 resolve_envs 相同合并语义）
#[tauri::command]
pub fn home_overview(state: State<'_, AppState>) -> Result<HomeOverview> {
    let chain = state.manager.chain(None)?;
    let paths = &state.manager.paths;

    let names: Vec<String> = state.manager.all_sdk_names();

    let mut tools = Vec::new();
    let mut merged: Vec<EnvVarRow> = Vec::new();
    for name in names {
        let Ok(tool) = state.manager.lookup_tool(&name) else { continue };
        let version = chain.tool(&name).map(|t| t.version().to_string());
        let active = crate::tool::is_active(&chain, &name, paths);

        // 与 resolve_envs 相同：env_keys 低优先级，extra_env_keys（插件/Lua）后并入覆盖
        let mut envs = tool.env_keys(paths, active);
        envs.extend(&tool.extra_env_keys(paths, active));

        for p in &envs.paths {
            merged.push(EnvVarRow {
                key: "PATH".into(),
                value: p.clone(),
                source: tool.desc().display().to_string(),
            });
        }
        let mut vars: Vec<EnvKeyValue> = envs
            .vars
            .iter()
            .filter(|(k, _)| !k.eq_ignore_ascii_case("PATH"))
            .map(|(k, v)| EnvKeyValue { key: k.clone(), value: v.clone().unwrap_or_default() })
            .collect();
        vars.sort_by(|a, b| a.key.cmp(&b.key));
        for v in &vars {
            merged.push(EnvVarRow {
                key: v.key.clone(),
                value: v.value.clone(),
                source: tool.desc().display().to_string(),
            });
        }
        // 图标：插件目录文件（svg>png>jpg）优先，其次插件声明 base64
        let icon = crate::tool::icon::resolve_icon(&paths.plugins.join(&name), tool.desc().icon_base64());
        tools.push(ToolEnvInfo {
            name,
            display: tool.desc().display().to_string(),
            category: tool.desc().category().to_string(),
            version,
            active,
            vars,
            path_entries: envs.paths.clone(),
            icon,
        });
    }

    // 全局 env（config.yaml [env]）并入
    let globals = state.manager.config.lock().unwrap().env_global().clone();
    let mut global_vars: Vec<EnvKeyValue> = globals
        .iter()
        .map(|(k, v)| EnvKeyValue { key: k.clone(), value: v.clone() })
        .collect();
    global_vars.sort_by(|a, b| a.key.cmp(&b.key));
    for v in &global_vars {
        merged.push(EnvVarRow {
            key: v.key.clone(),
            value: v.value.clone(),
            source: "全局配置".into(),
        });
    }

    // 排序：PATH 优先，其余按 key（同名按来源）升序
    merged.sort_by(|a, b| {
        let a_path = a.key == "PATH";
        let b_path = b.key == "PATH";
        match (a_path, b_path) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.key.cmp(&b.key).then(a.source.cmp(&b.source)),
        }
    });

    Ok(HomeOverview { tools, merged })
}
