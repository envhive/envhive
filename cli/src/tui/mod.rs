//! ratatui 交互界面：工具 / 插件 / 队列 / 镜像 / 统计 / 设置 / 关于。
//!
//! 按职责拆分为子模块：
//! - [`actions`]：键盘事件分发与 Tab 内导航（on_key / on_tab / move_sel 等）
//! - [`input`]：输入 / 确认模态框（处理 + 渲染）
//! - [`render`]：整体渲染主分发 + 状态栏 / 关于页
//! - [`tools`] / [`plugins`] / [`queue`] / [`mirror`] / [`stats`] / [`settings`]：
//!   各 Tab 的动作处理与页面渲染
//! - [`fmt`]：文本格式化与终端 cell 宽度工具
//!
//! 下载进度经 ChannelSink → mpsc channel 实时投递，不依赖 Tauri 事件。

mod actions;
mod fmt;
mod input;
mod mirror;
mod plugins;
mod queue;
mod render;
mod settings;
mod stats;
mod tools;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, KeyEventKind};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use envhive_core::config::RegistryEntry;
use envhive_core::error::Result;
use envhive_manager::events::{DownloadProgress, ManagerEvent};
use envhive_manager::extras::RemotePluginInfo;
use envhive_manager::manager::EnvHiveManager;
use envhive_manager::queue::{QueueManager, QueueTask};
use envhive_manager::usage::{stats as usage_stats, UsageStats};
use envhive_toolkit::plugin::PluginInfo;
use envhive_toolkit::registry::{PresetInfo, RegistryState};
use envhive_toolkit::tool::ToolInfo;

/// 镜像源管理涉及的工具（与桌面 NetworkPage 一致）
const REGISTRY_TOOLS: &[&str] = &["npm", "pip", "cargo", "maven", "go", "docker", "nuget", "gem", "pub", "conda"];

/// UI 消息：manager 事件 + 内部异步结果
#[derive(Clone)]
pub enum UiMsg {
    Manager(ManagerEvent),
    /// 版本列表拉取结果
    Versions { tool: String, versions: Vec<String> },
    /// 远程插件市场拉取结果
    RemotePlugins(Vec<RemotePluginInfo>),
    Status(String),
}

/// 文本输入目标（设置项 / 自定义源）
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum InputField {
    ProxyUrl,
    CacheTtl,
    StoragePath,
    /// 添加插件仓库：第 1 步仓库名
    RegistryName,
    /// 添加插件仓库：第 2 步 manifest 地址
    RegistryUrl,
    /// 添加自定义镜像源：第 1 步名称
    MirrorName,
    /// 添加自定义镜像源：第 2 步地址
    MirrorUrl,
}

/// 输入模态框状态
struct InputState {
    field: InputField,
    prompt: String,
    buffer: String,
    hint: String,
}

/// 危险操作确认
enum ConfirmAction {
    DeletePlugin(String),
    DeleteMirrorPreset { tool: String, name: String },
    UninstallVersion { tool: String, version: String },
    RemoveRegistryEntry(usize),
}

/// 确认模态框状态
struct ConfirmState {
    prompt: String,
    action: ConfirmAction,
}

pub struct TuiApp {
    manager: Arc<EnvHiveManager>,
    queue: Arc<QueueManager>,
    /// 异步任务回投消息的 sender（与 run 的 rx 同通道）
    tx: mpsc::UnboundedSender<UiMsg>,
    tab: usize, // 0 工具 / 1 插件 / 2 队列 / 3 镜像 / 4 统计 / 5 设置 / 6 关于
    // 工具 Tab
    tools: Vec<ToolInfo>,
    tool_idx: usize,
    focus_versions: bool,
    versions: Vec<String>,
    ver_idx: usize,
    // 插件 Tab：0=本地 1=市场
    plugin_view: usize,
    plugins: Vec<PluginInfo>,
    plugin_idx: usize,
    remote_plugins: Vec<RemotePluginInfo>,
    // 镜像 Tab：0=镜像源 1=下载加速
    mirror_view: usize,
    mirror_tools: Vec<String>,
    mirror_idx: usize,
    /// 镜像 Tab 焦点：true=左列（工具/SDK），false=右列（预设/镜像）
    focus_mirror_tools: bool,
    mirror_states: HashMap<String, RegistryState>,
    mirror_presets: HashMap<String, Vec<PresetInfo>>,
    preset_idx: usize,
    mirror_cfg: envhive_core::config::DownloadMirrorConfig,
    // 统计 Tab
    stats: Option<UsageStats>,
    stats_idx: usize,
    focus_stats_versions: bool,
    stats_ver_idx: usize,
    // 设置 Tab
    settings_entries: Vec<RegistryEntry>,
    settings_idx: usize,
    // 输入 / 确认模态
    input: Option<InputState>,
    confirm: Option<ConfirmState>,
    /// 添加插件仓库第 1 步暂存的名字
    pending_registry_name: Option<String>,
    /// 添加自定义镜像源第 1 步暂存的工具名
    pending_mirror: Option<String>,
    /// 添加自定义镜像源第 2 步暂存的镜像名
    pending_mirror_name: Option<String>,
    // 队列 Tab
    tasks: Vec<QueueTask>,
    task_idx: usize,
    // 状态
    progress: Option<DownloadProgress>,
    status: String,
    error: Option<String>,
    quitting: bool,
    /// 切 Tab 时设 true，下一次 draw 前 `Terminal::clear()` 真正清屏
    /// （彻底重置 ratatui buffer 与终端光标位置，修复 IDEA Terminal 等终端
    /// 差分渲染下的边框字符漂移/错位）。
    pending_clear: bool,
}

impl TuiApp {
    fn new(manager: Arc<EnvHiveManager>, queue: Arc<QueueManager>, tx: mpsc::UnboundedSender<UiMsg>) -> Self {
        let tools = manager.list_tools(None).unwrap_or_default();
        let plugins = manager.list_plugins();
        let tasks = queue.snapshot();
        let versions = TuiApp::versions_for(tools.first(), &[]);
        let cfg = manager.config.lock().unwrap().clone();
        let mut app = TuiApp {
            manager,
            queue,
            tx,
            tab: 0,
            tools,
            tool_idx: 0,
            focus_versions: false,
            versions,
            ver_idx: 0,
            plugin_view: 0,
            plugins,
            plugin_idx: 0,
            remote_plugins: Vec::new(),
            mirror_view: 0,
            mirror_tools: REGISTRY_TOOLS.iter().map(|s| s.to_string()).collect(),
            mirror_idx: 0,
            focus_mirror_tools: true,
            mirror_states: HashMap::new(),
            mirror_presets: HashMap::new(),
            preset_idx: 0,
            mirror_cfg: cfg.download_mirror.clone(),
            stats: None,
            stats_idx: 0,
            focus_stats_versions: false,
            stats_ver_idx: 0,
            settings_entries: cfg.registry.entries(),
            settings_idx: 0,
            input: None,
            confirm: None,
            pending_registry_name: None,
            pending_mirror: None,
            pending_mirror_name: None,
            tasks,
            task_idx: 0,
            progress: None,
            status: "就绪".into(),
            error: None,
            quitting: false,
            pending_clear: false,
        };
        app.refresh_mirror();
        app.refresh_stats();
        app.status = "就绪 · 1-7 切换 Tab，Tab 子视图焦点，q 退出".into();
        app
    }

    /// 初始版本列表：已装版本 + 已缓存可用版本（未拉取时仅已装）
    fn versions_for(tool: Option<&ToolInfo>, _extra: &[String]) -> Vec<String> {
        let Some(t) = tool else { return Vec::new() };
        let mut out = t.installed.clone();
        if let Some(avail) = &t.available {
            for v in avail {
                if !out.contains(v) {
                    out.push(v.clone());
                }
            }
        }
        out
    }

    pub async fn run(
        mut self,
        rx: &mut mpsc::UnboundedReceiver<UiMsg>,
    ) -> Result<()> {
        let mut terminal = ratatui::init();
        let mut stream = crossterm::event::EventStream::new();
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        loop {
            if self.quitting {
                break;
            }
            tokio::select! {
                evt = stream.next() => {
                    match evt {
                        Some(Ok(Event::Key(k))) if k.kind == KeyEventKind::Press => self.on_key(k.code),
                        Some(Ok(Event::Resize(..))) => {}
                        _ => {}
                    }
                }
                msg = rx.recv() => {
                    match msg {
                        Some(m) => self.on_msg(m),
                        None => break,
                    }
                }
                _ = tick.tick() => {
                    // 周期刷新：队列状态（安装进度）实时可见
                    self.tasks = self.queue.snapshot();
                }
            }
            // 切 Tab：真正清屏 + 重置 ratatui 内部 buffer，彻底解决 IDEA Terminal
            // 等终端下差分渲染的边框漂移/错位（仅切 Tab 时，零性能负担）。
            if self.pending_clear {
                terminal.clear()?;
                self.pending_clear = false;
            }
            terminal.draw(|f| self.render(f))?;
        }
        ratatui::restore();
        Ok(())
    }

    /// 供异步任务回投消息的 sender（与 run 的 rx 同通道）
    fn tx(&self) -> mpsc::UnboundedSender<UiMsg> {
        self.tx.clone()
    }

    /// 重拉工具列表（current / installed 变化后调用）。
    /// 只更新 `tools` 本身 —— 版本列表内容不变，标记（[全局]/[已装]）在渲染时
    /// 直接读 `tools[].current / .installed`，刷新后自然同步。
    fn refresh_tools(&mut self) {
        if let Ok(tools) = self.manager.list_tools(None) {
            if !tools.is_empty() && self.tool_idx >= tools.len() {
                self.tool_idx = tools.len() - 1;
            }
            self.tools = tools;
        }
    }

    fn on_msg(&mut self, msg: UiMsg) {
        // 安装/卸载等改变已装列表 → 顺带刷新统计（代价低，保证数据一致）
        let refresh_stats = matches!(msg, UiMsg::Manager(ManagerEvent::QueueUpdated(_)) | UiMsg::Status(_));
        match msg {
            UiMsg::Manager(evt) => match evt {
                ManagerEvent::Progress(p) => {
                    self.progress = Some(p);
                    self.tasks = self.queue.snapshot();
                }
                ManagerEvent::InstallStatus(s) => {
                    self.status = format!("{} {} {}", s.tool, s.version, s.detail.unwrap_or_default());
                }
                ManagerEvent::Error { tool, code, message } => {
                    self.error = Some(format!("[{tool}] {code}: {message}"));
                }
                ManagerEvent::VersionChanged(v) => {
                    self.status = format!("✓ {} 已切换为全局默认 {}", v.tool, v.version);
                    // 刷新工具数据：current/installed 变化 → 列表与版本标记同步更新
                    self.refresh_tools();
                }
                ManagerEvent::QueueUpdated(tasks) => {
                    self.tasks = tasks;
                    // 安装/取消收尾 → 已装列表变化，刷新标记
                    self.refresh_tools();
                }
            },
            UiMsg::Versions { tool, versions } => {
                self.versions = versions;
                self.ver_idx = 0;
                self.focus_versions = true;
                self.status = format!("{tool} 可用版本 {} 个（Tab 在版本列表导航）", self.versions.len());
            }
            UiMsg::RemotePlugins(list) => {
                self.remote_plugins = list;
                self.plugin_idx = 0;
                self.status = format!("插件市场共 {} 个插件（Enter 安装 / r 刷新）", self.remote_plugins.len());
            }
            UiMsg::Status(s) => self.status = s,
        }
        if refresh_stats {
            if let Ok(s) = usage_stats(&self.manager) {
                self.stats = Some(s);
            }
        }
    }
}

/// 启动 TUI：初始化 manager/queue 与事件通道
pub async fn run_tui(
    manager: Arc<EnvHiveManager>,
    queue: Arc<QueueManager>,
    tx: mpsc::UnboundedSender<UiMsg>,
    rx: mpsc::UnboundedReceiver<UiMsg>,
) -> Result<()> {
    let app = TuiApp::new(manager, queue, tx);
    let mut rx = rx;
    app.run(&mut rx).await
}
