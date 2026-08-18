//! envhive-cli —— 项目配置生成与终端环境注入
//!
//! 与桌面应用共存的双二进制：CLI 复用 envhive-core / envhive-toolkit（零 Tauri 依赖）。
//!
//! 命令：
//! - `envhive-cli init [--dir <path>] [--force] [--tool <name>=<version>]...`
//!   在指定（默认当前）项目目录生成 `.envhive.toml` 配置文件
//! - `envhive-cli load [--shell <bash|zsh|fish|powershell|cmd>] [--dir <path>] [--json]`
//!   从当前目录向上定位配置 → 计算合并 env → 输出对应 shell 语法的注入脚本
//!   （bash/zsh: `eval "$(envhive-cli load)"`；powershell: `envhive-cli load | Out-String | Invoke-Expression`；
//!    cmd: `envhive-cli load > %TEMP%\p.cmd && call %TEMP%\p.cmd`）

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use envhive_core::config::AppConfig;
use envhive_core::error::Result;
use envhive_core::pathmeta;
use envhive_core::toml_chain::{ConfigChain, ScopeConfig, ToolValue};
use envhive_toolkit::env_resolver::{resolve_envs, ToolLookup};
use envhive_toolkit::shell::{render, ShellKind};

/// 数据根目录：与桌面端一致（`~/.envhive`）
fn data_root() -> PathBuf {
    dirs_home().join(".envhive")
}

fn dirs_home() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

#[derive(Parser)]
#[command(name = "envhive-cli", version, about = "蜂巢 EnvHive CLI：项目配置生成与终端环境注入")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 在指定（默认当前）项目目录生成 .envhive.toml 配置文件
    Init {
        /// 目标目录（默认当前目录；不存在则自动创建）
        #[arg(long)]
        dir: Option<PathBuf>,
        /// 目标文件已存在时覆盖（默认拒绝）
        #[arg(long)]
        force: bool,
        /// 直接写入工具版本，如 --tool nodejs=22.11.0（可多次）
        #[arg(long = "tool", value_parser = parse_tool_value)]
        tools: Vec<(String, ToolValue)>,
    },
    /// 从当前目录向上定位配置，输出对应 shell 语法的环境注入脚本
    Load {
        /// 目标 shell（默认自动检测）
        #[arg(long)]
        shell: Option<String>,
        /// 从指定目录向上定位 .envhive.toml（默认当前目录）
        #[arg(long)]
        dir: Option<PathBuf>,
        /// 输出结构化 JSON（环境变量表 + 配置来源），供脚本/工具消费
        #[arg(long)]
        json: bool,
    },
}

/// `--tool nodejs=22.11.0` / `java=21,vendor=openjdk`
fn parse_tool_value(s: &str) -> Result<(String, ToolValue)> {
    let (name, spec) = s
        .split_once('=')
        .ok_or_else(|| envhive_core::error::EnvHiveError::new(
            envhive_core::error::EnvHiveErrorKind::Config,
            format!("--tool 格式应为 <name>=<version>，收到: {s}"),
        ))?;
    let name = name.trim().to_string();
    let version = spec.trim().to_string();
    if name.is_empty() || version.is_empty() {
        return Err(envhive_core::error::EnvHiveError::new(
            envhive_core::error::EnvHiveErrorKind::Config,
            format!("--tool 名称与版本不可为空: {s}"),
        ));
    }
    // 支持 `java=21,vendor=openjdk` 形式 → Attrs
    let value = if let Some((ver, attrs)) = version.split_once(',') {
        let mut vendor = None;
        let mut unlink = None;
        for kv in attrs.split(',').filter(|a| !a.is_empty()) {
            let (k, v) = kv
                .split_once('=')
                .ok_or_else(|| envhive_core::error::EnvHiveError::new(
                    envhive_core::error::EnvHiveErrorKind::Config,
                    format!("属性格式应为 key=value: {kv}"),
                ))?;
            match k.trim() {
                "vendor" => vendor = Some(v.trim().to_string()),
                "unlink" => unlink = Some(v.trim() == "true"),
                _ => {
                    return Err(envhive_core::error::EnvHiveError::new(
                        envhive_core::error::EnvHiveErrorKind::Config,
                        format!("未知属性 {k:?}（支持 vendor / unlink）"),
                    ))
                }
            }
        }
        ToolValue::Attrs { version: ver.trim().to_string(), vendor, unlink }
    } else {
        ToolValue::plain(version)
    };
    Ok((name, value))
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Init { dir, force, tools } => {
            if let Err(e) = cmd_init(dir, force, &tools) {
                eprintln!("错误: {e}");
                std::process::exit(1);
            }
        }
        Command::Load { shell, dir, json } => {
            if let Err(e) = cmd_load(shell.as_deref(), dir, json) {
                eprintln!("错误: {e}");
                std::process::exit(1);
            }
        }
    }
}

/// init：生成项目配置文件
fn cmd_init(dir: Option<PathBuf>, force: bool, tools: &[(String, ToolValue)]) -> Result<()> {
    let target_dir = dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let file = target_dir.join(".envhive.toml");

    if file.exists() && !force {
        eprintln!(
            "{} 已存在（使用 --force 覆盖；现有文件不会被修改）",
            file.display()
        );
        return Ok(());
    }

    // 构建配置：空 tools 时生成注释模板
    let mut cfg = ScopeConfig::default();
    for (name, value) in tools {
        cfg.tools.insert(name.clone(), value.clone());
    }

    // 写回（原子：临时文件 + rename）；目录不存在时自动创建
    cfg.save_to(&file)?;

    if tools.is_empty() {
        println!(
            "已生成 {}\n提示: 使用 `envhive-cli init --tool nodejs=22.11.0` 直接写入工具版本，\n     或 `envhive-cli load` 加载配置到当前终端。",
            file.display()
        );
    } else {
        println!("已生成 {}（{} 个工具）", file.display(), tools.len());
    }
    Ok(())
}

/// load：计算合并 env → 输出 shell 脚本 / JSON
fn cmd_load(shell_arg: Option<&str>, dir: Option<PathBuf>, json: bool) -> Result<()> {
    // 1. 数据根 + 路径元数据（不写磁盘）
    let root = data_root();
    let config_path = root.join("config.yaml");
    let config = AppConfig::load(&config_path).unwrap_or_default();
    let paths = pathmeta::from_root(&root);

    // 2. 配置链（从指定目录或 cwd 向上定位 .envhive.toml）
    let project_dir = dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let chain = ConfigChain::load(&paths, Some(&project_dir))?;

    // 3. 计算合并 env
    let lookup = ToolLookup::new();
    let envs = resolve_envs(&paths, &chain, config.env_global(), &lookup)?;

    // 4. 输出
    if json {
        output_json(&envs);
    } else {
        let shell = match shell_arg {
            Some(s) => ShellKind::parse(s).ok_or_else(|| {
                envhive_core::error::EnvHiveError::new(
                    envhive_core::error::EnvHiveErrorKind::Config,
                    format!("不支持的 shell: {s:?}（支持 bash / zsh / fish / powershell / cmd）"),
                )
            })?,
            None => ShellKind::detect(),
        };
        let script = render(&envs, shell);
        if script.is_empty() {
            println!("# 无环境注入（未在 .envhive.toml 中配置已安装的工具）");
        } else {
            println!("{script}");
        }
    }
    Ok(())
}

/// --json 输出：环境变量表 + PATH 条目
fn output_json(envs: &envhive_core::env::Envs) {
    let mut vars: Vec<serde_json::Value> = envs
        .vars
        .iter()
        .map(|(k, v)| {
            serde_json::json!({
                "key": k,
                "value": v,
                "action": if v.is_some() { "set" } else { "unset" },
            })
        })
        .collect();
    vars.sort_by(|a, b| a["key"].as_str().cmp(&b["key"].as_str()));

    let out = serde_json::json!({
        "path": envs.paths,
        "vars": vars,
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
}
