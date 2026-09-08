//! 队列 Tab：下载任务列表 + 当前任务进度条（Gauge）。

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph};

use envhive_manager::queue::TaskStatus;

use super::*;

impl TuiApp {
    pub(crate) fn cancel_task(&mut self) {
        let Some(t) = self.tasks.get(self.task_idx) else { return };
        match self.queue.cancel(t.id) {
            Ok(()) => self.status = format!("已请求取消任务 #{}", t.id),
            Err(e) => self.error = Some(format!("取消失败: {e}")),
        }
    }

    pub(crate) fn render_queue(&mut self, f: &mut ratatui::Frame<'_>, area: Rect) {
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
        self.click_lists.push((ClickTarget::TaskList, chunks[1]));
    }
}
