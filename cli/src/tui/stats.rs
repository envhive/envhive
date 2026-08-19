//! 统计 Tab：工具使用统计（次数 / 磁盘 / 上次使用）+ 版本明细。

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use envhive_manager::usage::{stats as usage_stats, ToolUsage, VersionUsage};

use super::*;

impl TuiApp {
    pub(crate) fn stats_tools(&self) -> Vec<&ToolUsage> {
        self.stats.as_ref().map(|s| s.tool_usage.iter().collect()).unwrap_or_default()
    }

    pub(crate) fn stats_versions(&self) -> Vec<&VersionUsage> {
        self.stats_tools()
            .get(self.stats_idx)
            .map(|t| t.versions.iter().collect())
            .unwrap_or_default()
    }

    pub(crate) fn refresh_stats(&mut self) {
        match usage_stats(&self.manager) {
            Ok(s) => {
                self.stats = Some(s);
                self.status = "统计已刷新".into();
            }
            Err(e) => self.error = Some(format!("统计失败: {e}")),
        }
    }

    pub(crate) fn confirm_uninstall_version(&mut self) {
        // 先拷贝出所需数据，结束 self 借用后再弹确认框
        let (tool, version, is_current, installed) = {
            let tools = self.stats_tools();
            let Some(t) = tools.get(self.stats_idx) else { return };
            let versions = t.versions.clone();
            let Some(v) = versions.get(self.stats_ver_idx) else { return };
            (t.tool.clone(), v.version.clone(), v.is_current, v.installed)
        };
        if is_current {
            self.status = "当前全局版本不可直接卸载，请先切换/解除全局使用".into();
            return;
        }
        if !installed {
            self.status = "该版本未安装".into();
            return;
        }
        self.confirm = Some(ConfirmState {
            prompt: format!("卸载 {tool} {version}？版本目录将被删除，可通过重新安装恢复。"),
            action: ConfirmAction::UninstallVersion { tool, version },
        });
    }

    pub(crate) fn render_stats(&self, f: &mut ratatui::Frame<'_>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        let body = chunks[0];

        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(body);

        // 左：工具汇总
        let items: Vec<ListItem> = self
            .stats_tools()
            .iter()
            .map(|t| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:<10}", t.tool), Style::default().fg(Color::Cyan)),
                    Span::styled(format!("{:<4}次", t.total_count), Style::default().fg(Color::Yellow)),
                    Span::raw(format!("{}", fmt::fmt_bytes(t.disk_bytes))),
                ]))
            })
            .collect();
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("使用统计（r 刷新）"))
            .highlight_style(if self.focus_stats_versions {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
            })
            .highlight_symbol("> ");
        let mut st = ListState::default();
        st.select(Some(self.stats_idx));
        f.render_stateful_widget(list, panes[0], &mut st);

        // 右：版本明细
        let tool = self.stats_tools().get(self.stats_idx).map(|t| t.tool.clone()).unwrap_or_default();
        let items: Vec<ListItem> = self
            .stats_versions()
            .iter()
            .map(|v| {
                let mut spans = vec![
                    Span::styled(format!("{:<14}", v.version), Style::default()),
                    Span::styled(format!("{:<4}次", v.count), Style::default().fg(Color::Yellow)),
                    Span::raw(format!("{:<10}", fmt::fmt_bytes(v.disk_bytes))),
                ];
                let used = v.last_used_days_ago.map(|d| format!("{d} 天前")).unwrap_or_else(|| "从未".into());
                spans.push(Span::styled(used, Style::default().fg(Color::DarkGray)));
                if v.is_current {
                    spans.push(Span::styled(" 全局", Style::default().fg(Color::Green)));
                } else if v.installed {
                    spans.push(Span::styled(" 已装", Style::default().fg(Color::DarkGray)));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(format!("版本 · {tool}（u 卸载）")))
            .highlight_style(if self.focus_stats_versions {
                Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().bg(Color::DarkGray)
            })
            .highlight_symbol("> ");
        let mut st = ListState::default();
        st.select(Some(self.stats_ver_idx));
        f.render_stateful_widget(list, panes[1], &mut st);

        // 底部汇总行
        if let Some(s) = &self.stats {
            let summary = Line::from(Span::styled(
                format!("合计: {} 个版本 · {} · 最近 30 天切换 {} 次", s.total_versions, fmt::fmt_bytes(s.total_disk_bytes), s.tool_usage.iter().map(|t| t.total_count).sum::<u32>()),
                Style::default().fg(Color::DarkGray),
            ));
            f.render_widget(Paragraph::new(summary), chunks[1]);
        }
    }
}
