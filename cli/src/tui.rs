//! ratatui 交互界面：工具 / 插件 / 队列 三 Tab。
//!
//! - 工具 Tab：左侧已注册工具列表（↑↓ 导航，Enter 拉取版本）；右侧版本列表
//!   （Enter 对选中版本操作：已装 → 切换全局，未装 → 入队安装）。
//! - 插件 Tab：插件列表，空格 启用/禁用。
//! - 队列 Tab：下载任务列表 + 当前任务进度条（Gauge），c 取消 / x 清空终态。
//! - 下载进度经 ChannelSink → mpsc channel 实时投递，不依赖 Tauri 事件。

use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Tabs};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use envhive_core::error::Result;
use envhive_manager::events::{DownloadProgress, ManagerEvent};
use envhive_manager::manager::EnvHiveManager;
use envhive_manager::queue::{QueueManager, QueueTask, TaskStatus};
use envhive_toolkit::plugin::PluginInfo;
use envhive_toolkit::tool::ToolInfo;

/// UI 消息：manager 事件 + 内部异步结果
#[derive(Clone)]
pub enum UiMsg {
    Manager(ManagerEvent),
    /// 版本列表拉取结果
    Versions { tool: String, versions: Vec<String> },
    Status(String),
}

pub struct TuiApp {
    manager: Arc<EnvHiveManager>,
    queue: Arc<QueueManager>,
    /// 异步任务回投消息的 sender（与 run 的 rx 同通道）
    tx: mpsc::UnboundedSender<UiMsg>,
    tab: usize, // 0 工具 / 1 插件 / 2 队列
    // 工具 Tab
    tools: Vec<ToolInfo>,
    tool_idx: usize,
    focus_versions: bool,
    versions: Vec<String>,
    ver_idx: usize,
    // 插件 Tab
    plugins: Vec<PluginInfo>,
    plugin_idx: usize,
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
        TuiApp {
            manager,
            queue,
            tx,
            tab: 0,
            tools,
            tool_idx: 0,
            focus_versions: false,
            versions,
            ver_idx: 0,
            plugins,
            plugin_idx: 0,
            tasks,
            task_idx: 0,
            progress: None,
            status: "就绪".into(),
            error: None,
            quitting: false,
            pending_clear: false,
        }
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

    // ------------------------------------------------------------------
    // 键盘 / 消息处理
    // ------------------------------------------------------------------

    fn on_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.quitting = true,
            KeyCode::Char('1') => { self.tab = 0; self.pending_clear = true; }
            KeyCode::Char('2') => { self.tab = 1; self.pending_clear = true; }
            KeyCode::Char('3') => { self.tab = 2; self.pending_clear = true; }
            KeyCode::Tab => {
                if self.tab == 0 {
                    self.focus_versions = !self.focus_versions;
                    self.status = if self.focus_versions { "版本列表" } else { "工具列表" }.into();
                } else {
                    self.tab = (self.tab + 1) % 3;
                    self.pending_clear = true;
                }
            }
            KeyCode::Up => self.move_sel(-1),
            KeyCode::Down => self.move_sel(1),
            KeyCode::Enter => self.on_enter(),
            KeyCode::Char(' ') => {
                if self.tab == 1 {
                    self.toggle_plugin();
                }
            }
            KeyCode::Char('c') => {
                if self.tab == 2 {
                    self.cancel_task();
                }
            }
            KeyCode::Char('x') => {
                if self.tab == 2 {
                    let n = self.queue.clear_finished();
                    self.status = format!("已清空 {n} 个已完成任务");
                }
            }
            _ => {}
        }
    }

    fn move_sel(&mut self, delta: isize) {
        match self.tab {
            0 => {
                if self.focus_versions {
                    self.ver_idx = shift(self.ver_idx, delta, self.versions.len());
                } else {
                    self.tool_idx = shift(self.tool_idx, delta, self.tools.len());
                    if self.tools.len() > self.tool_idx {
                        self.versions = TuiApp::versions_for(Some(&self.tools[self.tool_idx]), &[]);
                    }
                }
            }
            1 => self.plugin_idx = shift(self.plugin_idx, delta, self.plugins.len()),
            2 => self.task_idx = shift(self.task_idx, delta, self.tasks.len()),
            _ => {}
        }
    }

    fn on_enter(&mut self) {
        match self.tab {
            0 => {
                if self.focus_versions {
                    self.act_on_version();
                } else {
                    self.fetch_versions();
                }
            }
            1 => self.toggle_plugin(),
            2 => self.cancel_task(),
            _ => {}
        }
    }

    /// 拉取选中工具的版本列表（异步，结果经 channel 回投）
    fn fetch_versions(&mut self) {
        let Some(tool) = self.tools.get(self.tool_idx) else { return };
        let name = tool.name.clone();
        let mgr = self.manager.clone();
        let tx = self.tx();
        self.status = format!("正在拉取 {name} 版本…");
        tokio::spawn(async move {
            match mgr.fetch_versions(&name, None, true).await {
                Ok(cache) => {
                    let _ = tx.send(UiMsg::Versions { tool: name, versions: cache.version_strings() });
                }
                Err(e) => {
                    let _ = tx.send(UiMsg::Status(format!("拉取 {name} 版本失败: {e}")));
                }
            }
        });
    }

    /// 对选中版本操作：已装 → 切换全局；未装 → 入队安装
    fn act_on_version(&mut self) {
        let Some(tool) = self.tools.get(self.tool_idx) else { return };
        let Some(version) = self.versions.get(self.ver_idx) else { return };
        let installed = tool.installed.iter().any(|v| v == version);
        if installed {
            let name = tool.name.clone();
            let v = version.clone();
            let ok_msg = format!("✓ {name} {version} 已设为全局默认");
            let mgr = self.manager.clone();
            let tx = self.tx();
            self.status = format!("正在切换 {name} -> {version}…");
            tokio::spawn(async move {
                match mgr.switch_global(&name, &v).await {
                    Ok(_r) => {
                        let _ = tx.send(UiMsg::Status(ok_msg));
                    }
                    Err(e) => {
                        let _ = tx.send(UiMsg::Status(format!("切换失败: {e}")));
                    }
                }
            });
        } else {
            let outcome = self.queue.enqueue(tool.name.clone(), version.clone(), None);
            let reused = outcome.reused;
            if !reused {
                let q = self.queue.clone();
                let m = self.manager.clone();
                tokio::spawn(async move {
                    q.run_worker(m).await;
                });
            }
            let tip = if reused { "（队列已有相同任务，已复用）" } else { "" };
            self.status = format!("已入队安装 {} {version} {tip}。Tab 切到「3 队列」查看进度", tool.name);
        }
    }

    fn toggle_plugin(&mut self) {
        let Some(p) = self.plugins.get(self.plugin_idx) else { return };
        let name = p.name.clone();
        let enable = !p.enabled;
        match self.manager.toggle_plugin(&name, enable) {
            Ok(()) => {
                self.plugins = self.manager.list_plugins();
                self.status = format!("插件 {name} 已{}", if enable { "启用" } else { "禁用" });
            }
            Err(e) => self.status = format!("操作失败: {e}"),
        }
    }

    fn cancel_task(&mut self) {
        let Some(t) = self.tasks.get(self.task_idx) else { return };
        match self.queue.cancel(t.id) {
            Ok(()) => self.status = format!("已请求取消任务 #{}", t.id),
            Err(e) => self.status = format!("取消失败: {e}"),
        }
    }

    /// 供异步任务回投消息的 sender（与 run 的 rx 同通道）
    fn tx(&self) -> mpsc::UnboundedSender<UiMsg> {
        self.tx.clone()
    }

    fn on_msg(&mut self, msg: UiMsg) {
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
            UiMsg::Status(s) => self.status = s,
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

    // ------------------------------------------------------------------
    // 渲染
    // ------------------------------------------------------------------

    fn render(&mut self, f: &mut ratatui::Frame<'_>) {
        let area = f.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)])
            .split(area);

        let tabs = vec![
            Line::from(vec![
                Span::styled(" 1 工具 ", tab_style(self.tab == 0)),
                Span::raw(" "),
            ]),
            Line::from(vec![
                Span::styled(" 2 插件 ", tab_style(self.tab == 1)),
                Span::raw(" "),
            ]),
            Line::from(vec![
                Span::styled(" 3 队列 ", tab_style(self.tab == 2)),
                Span::raw(" "),
            ]),
        ];
        f.render_widget(Tabs::new(tabs).select(self.tab).block(Block::default().borders(Borders::ALL).title("envhive-cli")), chunks[0]);

        // 内容区先整体清空再重画：不同 Tab 的布局（列数/行数）差异大，
        // 部分终端（如 IDEA Terminal，Java 模拟器）对差分渲染的"光标定位 + 原地覆盖"
        // 支持不完整，旧边框 cell 会残留错位。Clear 强制以空格覆盖，兼容性兜底。
        f.render_widget(Clear, chunks[1]);

        match self.tab {
            0 => self.render_tools(f, chunks[1]),
            1 => self.render_plugins(f, chunks[1]),
            _ => self.render_queue(f, chunks[1]),
        }

        self.render_status(f, chunks[2]);
    }

    fn render_tools(&self, f: &mut ratatui::Frame<'_>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        // 左侧：工具列表
        let items: Vec<ListItem> = self
            .tools
            .iter()
            .map(|t| {
                let cur = t.current.clone().unwrap_or_else(|| "—".into());
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:<10}", t.name), Style::default().fg(Color::Cyan)),
                    Span::raw(" "),
                    Span::styled(cur, Style::default().fg(Color::Green)),
                ]))
            })
            .collect();
        let hl = if self.focus_versions { Style::default().bg(Color::DarkGray) } else { Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD) };
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("工具（Enter 拉取版本）"))
            .highlight_style(hl)
            .highlight_symbol("> ");
        let mut st = ListState::default();
        st.select(Some(self.tool_idx));
        f.render_stateful_widget(list, chunks[0], &mut st);

        // 右侧：版本列表
        let tool = self.tools.get(self.tool_idx).map(|t| t.name.clone()).unwrap_or_default();
        let items: Vec<ListItem> = self
            .versions
            .iter()
            .map(|v| {
                let installed = self
                    .tools
                    .get(self.tool_idx)
                    .map(|t| t.installed.iter().any(|iv| iv == v))
                    .unwrap_or(false);
                let global = self
                    .tools
                    .get(self.tool_idx)
                    .map(|t| t.current.as_deref() == Some(v.as_str()))
                    .unwrap_or(false);
                let tag = if global {
                    " [全局]".to_string()
                } else if installed {
                    " [已装]".to_string()
                } else {
                    " [安装]".to_string()
                };
                let color = if global { Color::Green } else if installed { Color::Yellow } else { Color::DarkGray };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:<14}", v), Style::default()),
                    Span::styled(tag, Style::default().fg(color)),
                ]))
            })
            .collect();
        let hl = if self.focus_versions { Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD) } else { Style::default().bg(Color::DarkGray) };
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(format!("版本 · {tool}")))
            .highlight_style(hl)
            .highlight_symbol("> ");
        let mut st = ListState::default();
        st.select(Some(self.ver_idx));
        f.render_stateful_widget(list, chunks[1], &mut st);
    }

    fn render_plugins(&self, f: &mut ratatui::Frame<'_>, area: Rect) {
        let items: Vec<ListItem> = self
            .plugins
            .iter()
            .map(|p| {
                let tag = if p.enabled { "启用" } else { "禁用" };
                let color = if p.enabled { Color::Green } else { Color::Red };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:<12}", p.name), Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{:<14}", p.display)),
                    Span::styled(tag, Style::default().fg(color)),
                    Span::raw(format!("  {}", p.source)),
                ]))
            })
            .collect();
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("插件（空格 启用/禁用）"))
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD))
            .highlight_symbol("> ");
        let mut st = ListState::default();
        st.select(Some(self.plugin_idx));
        f.render_stateful_widget(list, area, &mut st);
    }

    fn render_queue(&self, f: &mut ratatui::Frame<'_>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(0)])
            .split(area);

        // 顶部：当前执行任务进度条
        let running = self
            .tasks
            .iter()
            .find(|t| t.status == TaskStatus::Running)
            .or_else(|| self.tasks.iter().find(|t| t.status != TaskStatus::Done));
        if let Some(t) = running {
            let label = format!(
                "{} {}  {:>5.1}%  {}",
                t.tool,
                t.version,
                t.percent,
                t.message.clone().unwrap_or_default()
            );
            let ratio = (t.percent / 100.0).clamp(0.0, 1.0) as f64;
            let gauge = Gauge::default()
                .block(Block::default().borders(Borders::ALL).title("下载进度"))
                .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
                .ratio(ratio)
                .label(label);
            f.render_widget(gauge, chunks[0]);
        } else {
            let empty = Paragraph::new(Line::from("队列空闲")).block(Block::default().borders(Borders::ALL).title("下载进度"));
            f.render_widget(empty, chunks[0]);
        }

        // 任务列表
        let items: Vec<ListItem> = self
            .tasks
            .iter()
            .map(|t| {
                let (tag, color) = match t.status {
                    TaskStatus::Queued => ("排队", Color::DarkGray),
                    TaskStatus::Running => ("下载", Color::Cyan),
                    TaskStatus::Done => ("完成", Color::Green),
                    TaskStatus::Failed => ("失败", Color::Red),
                    TaskStatus::Cancelled => ("取消", Color::Yellow),
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("#{:<3}", t.id), Style::default().fg(Color::DarkGray)),
                    Span::styled(format!("{:<10}", t.tool), Style::default().fg(Color::Cyan)),
                    Span::styled(format!("{:<12}", t.version), Style::default()),
                    Span::styled(format!("{:<5}", tag), Style::default().fg(color)),
                    Span::styled(format!("{:>5.0}%", t.percent), Style::default()),
                    Span::raw(format!("  {}", t.message.clone().unwrap_or_default())),
                ]))
            })
            .collect();
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(format!("任务（{}）", self.tasks.len())))
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD))
            .highlight_symbol("> ");
        let mut st = ListState::default();
        st.select(Some(self.task_idx));
        f.render_stateful_widget(list, chunks[1], &mut st);
    }

    fn render_status(&self, f: &mut ratatui::Frame<'_>, area: Rect) {
        let (text, color) = match &self.error {
            Some(e) => (format!("✗ {e}"), Color::Red),
            None => (format!("{}", self.status), Color::Green),
        };
        let hint = " q 退出  1/2/3 Tab  ↑↓ 导航  Enter 操作  空格 启停  c 取消  x 清空";
        let line = Line::from(vec![
            Span::styled(text, Style::default().fg(color)),
            Span::raw("    "),
            Span::styled(hint, Style::default().fg(Color::DarkGray)),
        ]);
        f.render_widget(Paragraph::new(line), area);
    }
}

fn tab_style(active: bool) -> Style {
    if active {
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn shift(idx: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (idx as isize + delta).rem_euclid(len as isize) as usize
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
