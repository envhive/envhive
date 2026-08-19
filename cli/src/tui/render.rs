//! 整体渲染：顶部 Tab 栏 + 内容区分发 + 底部状态栏 + 关于页。

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};

use super::*;

impl TuiApp {
    pub(crate) fn render(&mut self, f: &mut ratatui::Frame<'_>) {
        let area = f.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)])
            .split(area);

        // 标签统一来自 TAB_TITLES（与鼠标命中检测同源，避免坐标错位）
        let mut tab_spans: Vec<Span> = Vec::new();
        for (i, title) in TAB_TITLES.iter().enumerate() {
            tab_spans.push(Span::styled(*title, fmt::tab_style(self.tab == i)));
            if i + 1 < TAB_TITLES.len() {
                tab_spans.push(Span::raw(" "));
            }
        }
        let tabs = vec![Line::from(tab_spans)];
        f.render_widget(Tabs::new(tabs).select(self.tab).block(Block::default().borders(Borders::ALL).title("envhive-cli")), chunks[0]);

        // 内容区先整体清空再重画：不同 Tab 的布局（列数/行数）差异大，
        // 部分终端（如 IDEA Terminal，Java 模拟器）对差分渲染的"光标定位 + 原地覆盖"
        // 支持不完整，旧边框 cell 会残留错位。Clear 强制以空格覆盖，兼容性兜底。
        f.render_widget(Clear, chunks[1]);

        match self.tab {
            0 => self.render_tools(f, chunks[1]),
            1 => self.render_plugins(f, chunks[1]),
            2 => self.render_queue(f, chunks[1]),
            3 => self.render_mirror(f, chunks[1]),
            4 => self.render_stats(f, chunks[1]),
            5 => self.render_settings(f, chunks[1]),
            _ => self.render_about(f, chunks[1]),
        }

        self.render_status(f, chunks[2]);

        // 模态框（输入 / 确认）覆盖绘制
        if self.input.is_some() {
            self.render_input(f, area);
        } else if self.confirm.is_some() {
            self.render_confirm(f, area);
        }
    }

    pub(crate) fn render_about(&self, f: &mut ratatui::Frame<'_>, area: Rect) {
        let paths = &self.manager.paths;
        let cfg = self.manager.config.lock().unwrap().clone();
        let writable = cfg.verify_writable().is_ok();
        let installed: usize = self.manager.list_installed().len();
        let lines: Vec<Line> = vec![
            Line::from(vec![Span::styled("envhive-cli · 蜂巢 EnvHive", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))]),
            Line::from(format!("版本      {}", env!("CARGO_PKG_VERSION"))),
            Line::from(format!("平台      {} / {}", std::env::consts::OS, std::env::consts::ARCH)),
            Line::from(format!("数据目录  {}", paths.root.display())),
            Line::from(format!("配置文件  {}（{}）", paths.config_file.display(), if writable { "可写" } else { "不可写" })),
            Line::from(format!("工具存储  {}", paths.install_root.display())),
            Line::from(format!("插件目录  {}", paths.plugins.display())),
            Line::from(format!("日志目录  {}", paths.logs.display())),
            Line::from(format!("已安装    {} 个插件 · {} 个版本", self.plugins.len(), installed)),
            Line::from(""),
            Line::from(Span::styled("项目: https://github.com/envhive/envhive（桌面端 + CLI 双形态）", Style::default().fg(Color::DarkGray))),
        ];
        let para = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("关于"));
        f.render_widget(para, area);
    }

    pub(crate) fn render_status(&self, f: &mut ratatui::Frame<'_>, area: Rect) {
        let (text, color) = match &self.error {
            Some(e) => (format!("✗ {e}"), Color::Red),
            None => (format!("{}", self.status), Color::Green),
        };
        let hint = match self.tab {
            0 => "q 退出  1-7/鼠标点 Tab  ↑↓ 导航  Enter 操作  Tab 焦点",
            1 => "m 本地/市场  空格 启停   i/Enter 安装   d/Del 删除   o 打开目录   r 刷新  ·鼠标点Tab",
            2 => "c 取消  x 清空终态  ·鼠标点Tab",
            3 => "m 源/加速  Enter 应用/切换  a 添加自定义  d/Del 删除自定义  r 刷新  ·鼠标点Tab",
            4 => "u 卸载  r 刷新  Tab 焦点  ·鼠标点Tab",
            5 => "空格 开关  e 编辑  a 添加仓库  d/Del 删除仓库  r 刷新  ·鼠标点Tab",
            _ => "全部功能一览  ·鼠标点Tab",
        };
        let line = Line::from(vec![
            Span::styled(text, Style::default().fg(color)),
            Span::raw("    "),
            Span::styled(hint, Style::default().fg(Color::DarkGray)),
        ]);
        f.render_widget(Paragraph::new(line), area);
    }
}
