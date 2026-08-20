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
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::Line;
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
use envhive_toolkit::tool::{DistributionInfo, ToolInfo};

/// 镜像源管理涉及的工具（与桌面 NetworkPage 一致）
const REGISTRY_TOOLS: &[&str] = &["npm", "pip", "cargo", "maven", "go", "docker", "nuget", "gem", "pub", "conda"];

/// 鼠标协议运行期自检：首个越界坐标即判定模拟器实现有缺陷，自动关闭鼠标。
struct MouseGuard {
    ok: bool,
}

impl MouseGuard {
    fn new() -> Self {
        Self { ok: true }
    }

    /// 校验一次鼠标坐标；越界（脏数据）返回 false 并标记失效。
    /// 合法点击必然落在终端范围内，因此单次越界即可判定，无需多次采样。
    fn check(&mut self, col: u16, row: u16, size: Rect) -> bool {
        if !self.ok {
            return false;
        }
        if col >= size.width || row >= size.height {
            self.ok = false;
        }
        self.ok
    }
}

/// 鼠标能力探测（不写死任何终端产品名，仅依据 `TERM` 做能力启发）。
///
/// - `dumb` / `unknown` 等明确无鼠标能力的终端 → 关
/// - xterm 系 / screen / tmux / rxvt / 任意 256color / linux → 声明支持鼠标协议 → 开
/// - 无 `TERM`（典型 Windows cmd / conhost）→ 乐观开启
///
/// 真正的实现缺陷由 [`MouseGuard`] 在运行期兜底关闭，故此处宁可乐观。
fn should_enable_mouse() -> bool {
    let t = match std::env::var("TERM") {
        Ok(t) => t,
        Err(_) => return true, // Windows cmd 等通常无 TERM，乐观开启
    };
    let t = t.to_ascii_lowercase();
    if t == "dumb" || t == "unknown" {
        return false;
    }
    t.starts_with("xterm")
        || t.starts_with("screen")
        || t.starts_with("tmux")
        || t.starts_with("rxvt")
        || t.contains("256color")
        || t == "linux"
}

/// 顶部 Tab 栏标签（与 render 共用，保证鼠标命中检测与显示完全一致）
pub(crate) const TAB_TITLES: &[&str] = &[
    " 1工具 ", " 2插件 ", " 3队列 ", " 4镜像 ", " 5统计 ", " 6设置 ", " 7关于 ",
];

/// Tab 标签之间的分隔符（渲染与鼠标命中检测共用，保证坐标对齐）。
/// 用 ASCII `|`，所有字符均为 1 cell 宽，避免 CJK 宽度计算偏差。
pub(crate) const TAB_SEP: &str = " | ";

/// 可点击列表区域标识（渲染期记录其外框 Rect，供鼠标命中检测）。
/// 与 [`TuiApp::hit_list`] / [`TuiApp::on_list_click`] 配合使用。
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ClickTarget {
    ToolList,
    VersionList,
    PluginList,
    TaskList,
    MirrorToolList,
    MirrorPresetList,
    StatsToolList,
    StatsVersionList,
    SettingList,
}

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
    /// 发行商维度（Lua 插件 TOOL.distributions 声明；仅含发行商维度的工具生效，如 Java）
    /// 无发行商维度（如 Node.js）时恒为 0，current_distribution_key() 返回 None。
    dist_idx: usize,
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
    // 设置项缓存：渲染期只读，变更时由 refresh_config 刷新（避免每帧加锁拷贝整个 config）
    proxy_enabled: bool,
    proxy_url: Option<String>,
    cache_ttl: String,
    storage_path: String,
    // 关于页：配置可写性缓存
    config_writable: bool,
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
    /// 运行期鼠标是否启用（协议异常时由 [`MouseGuard`] 自动关闭）
    mouse_enabled: bool,
    /// 鼠标协议自检：首个越界坐标即判定模拟器实现有缺陷
    mouse_guard: MouseGuard,
    /// 渲染期记录的可点击列表区域（外框 Rect + 标识），供鼠标命中检测。
    /// 每次 `render` 前清空、由各列表渲染方法重写，始终对应当前 Tab 布局。
    click_lists: Vec<(ClickTarget, Rect)>,
    /// 最近一次列表左键点击（目标 + 行 + 时刻），用于双击检测。
    /// 双击只在不超出阈值且同为同一行时触发动作，避免与单次选中冲突。
    last_click: Option<(ClickTarget, usize, Instant)>,
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
            dist_idx: 0,
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
            proxy_enabled: cfg.proxy.enable,
            proxy_url: cfg.proxy.url.clone(),
            cache_ttl: cfg.cache.available_hook_duration.clone(),
            storage_path: cfg.storage.tool_path.clone(),
            config_writable: cfg.verify_writable().is_ok(),
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
            mouse_enabled: false,
            mouse_guard: MouseGuard::new(),
            click_lists: Vec::new(),
            last_click: None,
        };
        app.refresh_mirror();
        app.refresh_stats();
        app.reset_distribution();
        app.status = "就绪 · 1-7/鼠标点 Tab，列表也可点击选中，Tab 切焦点，d 切换发行商，q 退出".into();
        app
    }

    /// 当前工具声明的发行商维度（无则 None）
    fn current_distributions(&self) -> Option<&[DistributionInfo]> {
        self.tools.get(self.tool_idx).and_then(|t| t.distributions.as_deref())
    }

    /// 当前选中的发行商 key（无发行商维度 → None，后续 effective_distribution 解析为无维度）
    fn current_distribution_key(&self) -> Option<String> {
        self.current_distributions()
            .and_then(|d| d.get(self.dist_idx).map(|x| x.key.clone()))
    }

    /// 当前发行商展示名（无维度 → None）
    fn current_distribution_display(&self) -> Option<String> {
        self.current_distributions()
            .and_then(|d| d.get(self.dist_idx).map(|x| x.display.clone()))
    }

    /// 切换工具后把发行商重置为该工具缺省（default_distribution 优先，否则取 [0]）；
    /// 无维度工具将 dist_idx 夹回 0。
    fn reset_distribution(&mut self) {
        let Some(dists) = self.current_distributions() else {
            self.dist_idx = 0;
            return;
        };
        let default = self
            .tools
            .get(self.tool_idx)
            .and_then(|t| t.default_distribution.as_deref());
        self.dist_idx = default
            .and_then(|d| dists.iter().position(|x| x.key == d))
            .unwrap_or(0);
    }

    /// 循环切换发行商（仅对含发行商维度的工具生效），并重新拉取版本列表。
    fn cycle_distribution(&mut self) {
        let dists = match self.current_distributions() {
            Some(d) if !d.is_empty() => d.to_vec(),
            _ => {
                self.status = "当前工具无发行商维度（如 Node.js 仅有单一官方源）".into();
                return;
            }
        };
        self.dist_idx = (self.dist_idx + 1) % dists.len();
        self.ver_idx = 0;
        let d = &dists[self.dist_idx];
        self.status = format!(
            "已切换发行商：{} （Enter 拉取版本 / 安装将使用该发行商）",
            d.display
        );
        self.fetch_versions();
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
        // 鼠标能力探测（不写死终端名）：仅明确不支持的类型才关，其余乐观开启，
        // 真正的协议缺陷由运行期 MouseGuard 自检兜底关闭。
        let mouse_enabled = should_enable_mouse();
        self.mouse_enabled = mouse_enabled;
        if mouse_enabled {
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::event::EnableMouseCapture
            );
        }
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
                        Some(Ok(Event::Mouse(m))) => {
                            if self.mouse_enabled {
                                // 仅响应左键按下；先判 Tab 栏，再判列表命中
                                if matches!(m.kind, MouseEventKind::Down(MouseButton::Left)) {
                                    if let Ok(size) = terminal.size() {
                                        // 运行期自检：首个越界坐标即判定该终端鼠标协议实现有缺陷 → 自动禁用
                                        if !self.mouse_guard.check(m.column, m.row, size.into()) {
                                            self.disable_mouse();
                                            self.status = "⚠ 当前终端鼠标协议异常，已自动禁用鼠标（用 1-7 切换 Tab）".into();
                                        } else if let Some(t) =
                                            TuiApp::tab_at(m.column, m.row, TuiApp::tab_bar_rect(size.into()))
                                        {
                                            self.switch_tab(t);
                                        } else if let Some((target, idx)) = self.hit_list(m.column, m.row) {
                                            // 双击检测：同一列表同一行、间隔 < 400ms 视为双击。
                                            // 双击镜像右侧预设/加速项即应用为当前（等同 Enter）；
                                            // 其他列表双击退化为普通选中，避免误触动作。
                                            let now = Instant::now();
                                            let is_double = match &self.last_click {
                                                Some((t, i, at)) => {
                                                    *t == target
                                                        && *i == idx
                                                        && now.duration_since(*at) < Duration::from_millis(400)
                                                }
                                                None => false,
                                            };
                                            if is_double {
                                                self.on_list_double_click(target, idx);
                                                self.last_click = None;
                                            } else {
                                                self.on_list_click(target, idx);
                                                self.last_click = Some((target, idx, now));
                                            }
                                        }
                                    }
                                }
                            }
                        }
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
        // 退出前关闭鼠标捕获，恢复终端默认行为（否则鼠标选择/复制会失效）
        if self.mouse_enabled {
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::event::DisableMouseCapture
            );
        }
        ratatui::restore();
        Ok(())
    }

    /// 供异步任务回投消息的 sender（与 run 的 rx 同通道）
    fn tx(&self) -> mpsc::UnboundedSender<UiMsg> {
        self.tx.clone()
    }

    /// 运行时关闭鼠标捕获并标记失效（仅执行一次）。
    fn disable_mouse(&mut self) {
        if self.mouse_enabled {
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::event::DisableMouseCapture
            );
            self.mouse_enabled = false;
        }
    }

    /// 由终端尺寸推导顶部 Tab 栏 Rect（与 render 的纵向布局一致：3 / Min / 3）
    fn tab_bar_rect(size: Rect) -> Rect {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)])
            .split(size)[0]
    }

    /// 命中检测：给定终端坐标，返回落在哪个 Tab 标签上（仅 Tab 栏那一行）。
    /// 与 render 用同一套 TAB_TITLES，坐标算法一致：内容区左移 1 列（左边框），
    /// 标签绘制在 bar.y+1（带边框 Block 的首行内容）；标题行 bar.y 也一并接受（容错）。
    pub(crate) fn tab_at(col: u16, row: u16, bar: Rect) -> Option<usize> {
        let top = bar.y + 1;
        if row != top && row != bar.y {
            return None;
        }
        let sep_w = Line::from(TAB_SEP).width() as u16;
        let mut x = bar.x + 1; // 跳过左边框
        for (i, title) in TAB_TITLES.iter().enumerate() {
            // Line::from(title).width() 按显示宽度计（CJK 计 2），与渲染一致
            let w = Line::from(*title).width() as u16;
            // 标签 + 其后分隔符都算可点击，避免点空
            let end = x + w + sep_w;
            if col >= x && col < end {
                return Some(i);
            }
            x = end;
        }
        None
    }

    /// 当前 Tab 中某可点击列表的元素数量（用于点击命中后的索引裁剪）。
    fn list_count(&self, target: ClickTarget) -> usize {
        match target {
            ClickTarget::ToolList => self.tools.len(),
            ClickTarget::VersionList => self.versions.len(),
            ClickTarget::PluginList => {
                if self.plugin_view == 1 { self.remote_plugins.len() } else { self.plugins.len() }
            }
            ClickTarget::TaskList => self.tasks.len(),
            ClickTarget::MirrorToolList => {
                if self.mirror_view == 1 { self.mirror_sdk_tools().len() } else { self.mirror_tools.len() }
            }
            ClickTarget::MirrorPresetList => {
                if self.mirror_view == 1 { self.mirror_options().len() } else { self.current_presets().len() }
            }
            ClickTarget::StatsToolList => self.stats_tools().len(),
            ClickTarget::StatsVersionList => self.stats_versions().len(),
            ClickTarget::SettingList => self.settings_rows(),
        }
    }

    /// 命中检测：给定终端坐标，返回落在哪个列表的哪一行（内容区内）。
    /// `click_lists` 由渲染期写入，坐标算法与渲染一致：`Block::borders(ALL)`
    /// 使内框相对外框上下左右各缩进 1 格，列表项从内框首行起逐行排列。
    fn hit_list(&self, col: u16, row: u16) -> Option<(ClickTarget, usize)> {
        for (target, rect) in &self.click_lists {
            let inner = Rect {
                x: rect.x + 1,
                y: rect.y + 1,
                width: rect.width.saturating_sub(2),
                height: rect.height.saturating_sub(2),
            };
            if col >= inner.x && col < inner.x + inner.width && row >= inner.y && row < inner.y + inner.height {
                let count = self.list_count(*target);
                if count == 0 {
                    return None;
                }
                let idx = (row - inner.y) as usize;
                if idx < count {
                    return Some((*target, idx));
                }
            }
        }
        None
    }

    /// 列表单击：选中对应行，并把焦点切到该列（双列 Tab 的左右列）。
    /// 只做选择、不做破坏性动作（安装/卸载/启停），避免误触；需要操作请按 Enter / 空格。
    fn on_list_click(&mut self, target: ClickTarget, idx: usize) {
        match target {
            ClickTarget::ToolList => {
                self.tool_idx = idx;
                self.focus_versions = false;
                if let Some(t) = self.tools.get(idx) {
                    self.versions = TuiApp::versions_for(Some(t), &[]);
                }
                self.reset_distribution();
            }
            ClickTarget::VersionList => {
                self.ver_idx = idx;
                self.focus_versions = true;
            }
            ClickTarget::PluginList => self.plugin_idx = idx,
            ClickTarget::TaskList => self.task_idx = idx,
            ClickTarget::MirrorToolList => {
                self.mirror_idx = idx;
                self.focus_mirror_tools = true;
                if self.mirror_view == 0 {
                    self.preset_idx = 0;
                }
            }
            ClickTarget::MirrorPresetList => {
                self.preset_idx = idx;
                self.focus_mirror_tools = false;
            }
            ClickTarget::StatsToolList => {
                self.stats_idx = idx;
                self.focus_stats_versions = false;
                self.stats_ver_idx = 0;
            }
            ClickTarget::StatsVersionList => {
                self.stats_ver_idx = idx;
                self.focus_stats_versions = true;
            }
            ClickTarget::SettingList => self.settings_idx = idx,
        }
        self.status = "鼠标已选中列表项（Enter 操作 / 空格 开关）".into();
    }

    /// 列表双击：等同该列表「Enter」的激活语义，但镜像/统计的**左侧**列表
    /// 双击仅选中——避免误触「应用预设 / 卸载工具」这类动作（这些仍走键盘 Enter）。
    fn on_list_double_click(&mut self, target: ClickTarget, idx: usize) {
        match target {
            // 工具：拉取该工具可用版本（等同 Enter 焦点在工具列表）
            ClickTarget::ToolList => {
                self.tool_idx = idx;
                self.focus_versions = false;
                if let Some(t) = self.tools.get(idx) {
                    self.versions = TuiApp::versions_for(Some(t), &[]);
                }
                self.reset_distribution();
                self.fetch_versions();
            }
            // 版本：切换该版本为全局默认
            ClickTarget::VersionList => {
                self.ver_idx = idx;
                self.focus_versions = true;
                self.act_on_version();
            }
            // 插件：本地启停 / 市场安装
            ClickTarget::PluginList => {
                self.plugin_idx = idx;
                if self.plugin_view == 1 {
                    self.install_remote_plugin();
                } else {
                    self.toggle_plugin();
                }
            }
            // 队列：取消该任务
            ClickTarget::TaskList => {
                self.task_idx = idx;
                self.cancel_task();
            }
            // 镜像右侧预设/加速：应用为当前
            ClickTarget::MirrorPresetList => {
                self.preset_idx = idx;
                self.focus_mirror_tools = false;
                self.activate_mirror();
            }
            // 统计版本：卸载该版本（带确认）
            ClickTarget::StatsVersionList => {
                self.stats_ver_idx = idx;
                self.focus_stats_versions = true;
                self.confirm_uninstall_version();
            }
            // 设置项：打开编辑（等同 Enter；开关行会切换，仓库行无操作）
            ClickTarget::SettingList => {
                self.settings_idx = idx;
                self.activate_setting();
            }
            // 镜像/统计左侧列表双击仅选中：避免误触「应用预设 / 卸载工具」
            ClickTarget::MirrorToolList | ClickTarget::StatsToolList => {
                self.on_list_click(target, idx);
            }
        }
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
                let dist = self
                    .current_distribution_display()
                    .map(|d| format!(" · {d}"))
                    .unwrap_or_default();
                self.status = format!("{tool}{dist} 可用版本 {} 个（Tab 在版本列表导航）", self.versions.len());
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
