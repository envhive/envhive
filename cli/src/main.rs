//! envhive-cli —— 独立工具（零 Tauri 依赖）
//!
//! 与桌面应用共存的双二进制：CLI 复用 envhive-core / envhive-toolkit / envhive-manager。
//!
//! 命令：
//! - `envhive-cli init [--dir <path>] [--force] [--tool <name>=<version>]...`
//!   在指定（默认当前）项目目录生成 `.envhive.toml` 配置文件
//! - `envhive-cli load [--shell <bash|zsh|fish|powershell|cmd>] [--dir <path>] [--json]`
//!   从当前目录向上定位配置 → 计算合并 env → 输出对应 shell 语法的注入脚本
//! - `envhive-cli tui`   交互式终端界面（ratatui）：工具安装 / 切换 / 插件 / 队列进度
//! - `envhive-cli install <name> <version>`  安装工具（行内进度条）
//! - `envhive-cli switch <name> <version>`   切换全局版本
//! - `envhive-cli unuse <name>`              解除全局使用
//! - `envhive-cli list`                      列出已安装工具

mod sink;
mod tui;

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tokio::sync::mpsc;

use envhive_core::config::AppConfig;
use envhive_core::error::Result;
use envhive_core::pathmeta;
use envhive_core::toml_chain::{ConfigChain, ScopeConfig, ToolValue};
use envhive_manager::events::{DownloadProgress, EventSink, ManagerEvent, NullSink};
use envhive_manager::manager::EnvHiveManager;
use envhive_manager::queue::QueueManager;
use envhive_toolkit::env_resolver::{resolve_envs, ToolLookup};
use envhive_toolkit::shell::{render, ShellKind};

use crate::sink::{ChannelSink, UiSink};
use crate::tui::UiMsg;

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

/// 构建 HTTP 客户端（与桌面一致：代理第 1 层）
fn build_http_client(cfg: &AppConfig) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .user_agent("envhive-cli/0.1.0")
        .connect_timeout(std::time::Duration::from_secs(15));
    if cfg.proxy.enable {
        if let Some(url) = &cfg.proxy.url {
            if let Ok(proxy) = reqwest::Proxy::all(url) {
                builder = builder.proxy(proxy);
            }
        }
    }
    builder.build().unwrap_or_default()
}

/// 构造 EnvHiveManager（注入事件接收器）
fn build_manager(sink: Arc<dyn EventSink>) -> EnvHiveManager {
    let root = data_root();
    let config = AppConfig::load(&root.join("config.yaml")).unwrap_or_default();
    let paths = pathmeta::from_root(&root);
    let client = build_http_client(&config);
    EnvHiveManager::with_event_sink(config, paths, client, sink)
}

#[derive(Parser)]
#[command(name = "envhive-cli", version, about = "蜂巢 EnvHive CLI：项目配置 / 终端环境注入 / 交互式工具管理")]
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
    /// 交互式终端界面（ratatui）：工具安装 / 切换 / 插件管理 / 队列进度
    Tui,
    /// 安装工具（非交互，终端行内显示下载进度）
    Install {
        /// 工具名（如 nodejs）
        name: String,
        /// 版本（精确或标签，如 22.11.0 / lts）
        version: String,
    },
    /// 切换全局默认版本（需已安装）
    Switch {
        /// 工具名（如 nodejs）
        name: String,
        /// 目标版本（精确或标签）
        version: String,
    },
    /// 解除全局使用（不卸载）
    Unuse {
        /// 工具名（如 nodejs）
        name: String,
    },
    /// 列出已安装工具
    List,
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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Init { dir, force, tools } => cmd_init(dir, force, &tools),
        Command::Load { shell, dir, json } => cmd_load(shell.as_deref(), dir, json),
        Command::Tui => cmd_tui().await,
        Command::Install { name, version } => cmd_install(&name, &version).await,
        Command::Switch { name, version } => cmd_switch(&name, &version).await,
        Command::Unuse { name } => cmd_unuse(&name),
        Command::List => cmd_list(),
    };
    if let Err(e) = result {
        eprintln!("错误: {e}");
        std::process::exit(1);
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

/// tui：交互式终端界面（ratatui）
async fn cmd_tui() -> Result<()> {
    let (tx, rx) = mpsc::unbounded_channel::<UiMsg>();
    // UiSink 把 manager 事件包装为 UiMsg::Manager 投递到 UI 通道
    let tx_manager = tx.clone();
    let manager = Arc::new(build_manager(Arc::new(UiSink(tx_manager))));
    let queue = Arc::new(QueueManager::with_event_sink(Arc::new(UiSink(tx.clone()))));
    crate::tui::run_tui(manager, queue, tx, rx).await
}

/// install：安装工具（行内进度条）
async fn cmd_install(name: &str, version: &str) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<ManagerEvent>();
    let manager = Arc::new(build_manager(Arc::new(ChannelSink(tx))));

    let mgr = manager.clone();
    let name_c = name.to_string();
    let ver_c = version.to_string();
    let mut handle = tokio::spawn(async move { mgr.install_tool(&name_c, &ver_c, None, None).await });

    // 消费进度事件 → 行内刷新
    loop {
        tokio::select! {
            evt = rx.recv() => {
                match evt {
                    Some(ManagerEvent::Progress(p)) => render_inline_progress(&p),
                    Some(ManagerEvent::Error { message, .. }) => {
                        eprintln!("\n错误: {message}");
                    }
                    Some(ManagerEvent::InstallStatus(s)) => {
                        if let Some(d) = &s.detail {
                            render_inline_stage(&s.tool, &s.version, d);
                        }
                    }
                    _ => {}
                }
            }
            res = &mut handle => {
                let result = res.map_err(|e| {
                    envhive_core::error::EnvHiveError::new(
                        envhive_core::error::EnvHiveErrorKind::Internal,
                        format!("安装任务异常: {e}"),
                    )
                })??;
                println!();
                println!("✓ {} {}（{}）", result.tool, result.version, result.path);
                return Ok(());
            }
        }
    }
}

/// switch：切换全局默认版本
async fn cmd_switch(name: &str, version: &str) -> Result<()> {
    let manager = build_manager(Arc::new(NullSink));
    let r = manager.switch_global(name, version).await?;
    println!("✓ {}", r.message);
    Ok(())
}

/// unuse：解除全局使用
fn cmd_unuse(name: &str) -> Result<()> {
    let manager = build_manager(Arc::new(NullSink));
    manager.unuse_global(name)?;
    println!("✓ {name} 已解除全局使用");
    Ok(())
}

/// list：列出已安装工具
fn cmd_list() -> Result<()> {
    let manager = build_manager(Arc::new(NullSink));
    let infos = manager.list_installed();
    if infos.is_empty() {
        println!("（未安装任何工具）");
        return Ok(());
    }
    for i in infos {
        let mark = if i.is_current { "*" } else { " " };
        println!("{mark} {:<12} {:<14} {}", i.tool, i.version, i.path);
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

/// 行内进度条（crossterm）：`[=====>     ] 45.0% 12.30 MB/s 下载中`
fn render_inline_progress(p: &DownloadProgress) {
    use std::io::Write;
    let width: usize = 36;
    let filled = ((p.percent / 100.0) * width as f32).round() as usize;
    let bar: String = format!(
        "[{}{}] {:>5.1}% {:>6.2} MB/s {}",
        "=".repeat(filled),
        " ".repeat(width.saturating_sub(filled)),
        p.percent,
        p.speed_mbps,
        stage_label(p.stage)
    );
    print!("\r{:<16} {:<12} {bar}", p.tool, p.version);
    let _ = std::io::stdout().flush();
}

fn render_inline_stage(tool: &str, version: &str, detail: &str) {
    use std::io::Write;
    print!("\r{:<16} {:<12} {detail:<40}", tool, version);
    let _ = std::io::stdout().flush();
}

fn stage_label(s: envhive_manager::events::DownloadStage) -> &'static str {
    use envhive_manager::events::DownloadStage as S;
    match s {
        S::Resolving => "解析",
        S::Downloading => "下载中",
        S::Verifying => "校验",
        S::Extracting => "解压安装",
        S::Done => "完成",
        S::Failed => "失败",
    }
}
