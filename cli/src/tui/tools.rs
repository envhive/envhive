//! 工具 Tab：左侧工具列表 + 右侧版本列表。
//!
//! Enter 左侧拉取版本列表（异步，结果经 channel 回投）；Enter 右侧对选中版本
//! 操作：已装 → 切换全局默认，未装 → 入队安装。

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use super::*;

impl TuiApp {
    /// 拉取选中工具的版本列表（异步，结果经 channel 回投）
    pub(crate) fn fetch_versions(&mut self) {
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
    pub(crate) fn act_on_version(&mut self) {
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

    pub(crate) fn render_tools(&mut self, f: &mut ratatui::Frame<'_>, area: Rect) {
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
        self.click_lists.push((ClickTarget::ToolList, chunks[0]));

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
        self.click_lists.push((ClickTarget::VersionList, chunks[1]));
    }
}
