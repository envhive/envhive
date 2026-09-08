//! 插件 Tab：本地插件列表（空格 启停 / d 删除 / o 打开目录）+ 远程市场视图。

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use envhive_manager::extras;

use super::*;

impl TuiApp {
    pub(crate) fn toggle_plugin(&mut self) {
        let Some(p) = self.plugins.get(self.plugin_idx) else { return };
        let name = p.name.clone();
        let enable = !p.enabled;
        match self.manager.toggle_plugin(&name, enable) {
            Ok(()) => {
                self.plugins = self.manager.list_plugins();
                self.status = format!("插件 {name} 已{}", if enable { "启用" } else { "禁用" });
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub(crate) fn open_plugin_dir(&mut self) {
        let Some(p) = self.plugins.get(self.plugin_idx) else { return };
        match self.manager.open_plugin_dir(&p.name) {
            Ok(()) => self.status = format!("已在文件管理器中打开 {}", p.path),
            Err(e) => self.error = Some(format!("打开目录失败: {e}")),
        }
    }

    /// 拉取远程插件市场（异步）
    pub(crate) fn fetch_remote_plugins(&mut self) {
        let mgr = self.manager.clone();
        let tx = self.tx();
        let cfg = self.manager.config.lock().unwrap();
        let address = cfg.registry.selected.clone()
            .or_else(|| cfg.registry.urls().into_iter().next())
            .unwrap_or_default();
        drop(cfg);
        self.status = format!("正在拉取插件市场…（{}）", address);
        tokio::spawn(async move {
            let client = mgr.client();
            match extras::fetch_remote_manifest(&address, &client).await {
                Ok(list) => {
                    let _ = tx.send(UiMsg::RemotePlugins(list));
                }
                Err(e) => {
                    let _ = tx.send(UiMsg::Status(format!("拉取插件市场失败: {e}")));
                }
            }
        });
    }

    pub(crate) fn install_remote_plugin(&mut self) {
        let Some(p) = self.remote_plugins.get(self.plugin_idx) else { return };
        let mgr = self.manager.clone();
        let plugin = p.clone();
        let tx = self.tx();
        self.status = format!("正在安装插件 {}@{}{}…", p.name, p.version, if p.format.eq_ignore_ascii_case("zip") { "（zip）" } else { "" });
        tokio::spawn(async move {
            let paths = mgr.paths.clone();
            let client = mgr.client();
            match extras::install_remote_plugin(&paths, &plugin, &client).await {
                Ok(()) => {
                    let _ = tx.send(UiMsg::Status(format!("✓ 插件 {} 已安装至 v{}", plugin.name, plugin.version)));
                }
                Err(e) => {
                    let _ = tx.send(UiMsg::Status(format!("安装插件失败: {e}")));
                }
            }
        });
    }

    pub(crate) fn render_plugins(&mut self, f: &mut ratatui::Frame<'_>, area: Rect) {
        if self.plugin_view == 1 {
            // 市场视图
            let items: Vec<ListItem> = self
                .remote_plugins
                .iter()
                .map(|p| {
                    let local = self.plugins.iter().find(|x| x.name == p.name);
                    let tag = match local {
                        Some(l) if l.version.as_deref() == Some(p.version.as_str()) => "已安装".to_string(),
                        Some(_) => "可更新".to_string(),
                        None => "未安装".to_string(),
                    };
                    let color = if local.is_some() { Color::Green } else { Color::DarkGray };
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{:<12}", p.name), Style::default().fg(Color::Cyan)),
                        Span::styled(format!("v{:<10}", p.version), Style::default()),
                        Span::styled(format!("{:<6}", tag), Style::default().fg(color)),
                        Span::raw(format!("{}", p.description)),
                    ]))
                })
                .collect();
            let title = if self.remote_plugins.is_empty() {
                "插件市场（r 拉取）".to_string()
            } else {
                format!("插件市场 · {} 个（Enter 安装 / r 刷新）", self.remote_plugins.len())
            };
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(title))
                .highlight_style(Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD))
                .highlight_symbol("> ");
            let mut st = ListState::default();
            st.select(Some(self.plugin_idx));
            f.render_stateful_widget(list, area, &mut st);
            self.click_lists.push((ClickTarget::PluginList, area));
            return;
        }

        // 本地视图
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
            .block(Block::default().borders(Borders::ALL).title("本地插件（空格 启停 / d/Del 删除 / o 打开目录 / m 市场）"))
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD))
            .highlight_symbol("> ");
        let mut st = ListState::default();
        st.select(Some(self.plugin_idx));
        f.render_stateful_widget(list, area, &mut st);
        self.click_lists.push((ClickTarget::PluginList, area));
    }
}
