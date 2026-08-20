//! 镜像 Tab：镜像源视图（按工具切换 registry 预设 + 自定义源增删）+
//! 下载加速视图（按 SDK 切换下载加速镜像）。

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use envhive_toolkit::registry::{presets_info, PresetInfo, RegistryState};

use super::*;

impl TuiApp {
    pub(crate) fn current_mirror_tool(&self) -> String {
        self.mirror_tools.get(self.mirror_idx).cloned().unwrap_or_default()
    }

    pub(crate) fn current_presets(&self) -> Vec<PresetInfo> {
        self.mirror_presets.get(&self.current_mirror_tool()).cloned().unwrap_or_default()
    }

    fn current_state(&self) -> Option<&RegistryState> {
        self.mirror_states.get(&self.current_mirror_tool())
    }

    /// 声明了下载加速镜像的 SDK 工具列表（仅这些可切换加速镜像）
    pub(crate) fn mirror_sdk_tools(&self) -> Vec<&ToolInfo> {
        self.tools
            .iter()
            .filter(|t| t.mirrors.as_ref().is_some_and(|m| !m.is_empty()))
            .collect()
    }

    /// 下载加速视图：某 SDK 的镜像选项（官方源 + 插件声明的镜像）
    pub(crate) fn mirror_options(&self) -> Vec<String> {
        let mut out = vec!["官方源".to_string()];
        if let Some(t) = self.mirror_sdk_tools().get(self.mirror_idx) {
            for m in t.mirrors.iter().flatten() {
                if !out.contains(&m.name) {
                    out.push(m.name.clone());
                }
            }
        }
        out
    }

    pub(crate) fn current_mirror_selected(&self) -> Option<String> {
        self.mirror_sdk_tools()
            .get(self.mirror_idx)
            .and_then(|t| self.current_mirror_of(t))
    }

    pub(crate) fn refresh_mirror(&mut self) {
        let mgr = self.manager.clone();
        // 只锁一次：同时取出下载镜像配置与自定义 registry（供下方 presets 复用）
        let (download_mirror, customs) = {
            let cfg = mgr.config.lock().unwrap();
            (cfg.download_mirror.clone(), cfg.custom_registry.clone())
        };
        self.mirror_cfg = download_mirror;
        // 直接迭代引用，避免整 Vec 克隆；仅 map 插入时克隆所需 String
        for tool in &self.mirror_tools {
            if let Ok(st) = mgr.registry_state(tool) {
                self.mirror_states.insert(tool.clone(), st);
            }
            let presets = presets_info(tool, &customs);
            self.mirror_presets.insert(tool.clone(), presets);
        }
        self.status = format!("镜像配置已刷新（{} 个工具）", self.mirror_tools.len());
    }

    /// 应用镜像源预设 / 切换下载加速镜像（按当前子视图）
    pub(crate) fn activate_mirror(&mut self) {
        if self.mirror_view == 1 {
            // 先拷贝出工具名/显示名，避免临时 Vec 借用 self 导致后续可变借用冲突
            let (name, display) = match self.mirror_sdk_tools().get(self.mirror_idx) {
                Some(t) => (t.name.clone(), t.display.clone()),
                None => return,
            };
            let options = self.mirror_options();
            let Some(sel) = options.get(self.preset_idx).cloned() else { return };
            let mirror = if sel == "官方源" { None } else { Some(sel.as_str()) };
            match self.manager.set_tool_mirror(&name, mirror) {
                Ok(cfg) => {
                    self.mirror_cfg = cfg;
                    self.status = if mirror.is_some() {
                        format!("{display} 下载加速镜像已切换为「{sel}」")
                    } else {
                        format!("{display} 已恢复官方源")
                    };
                }
                Err(e) => self.error = Some(format!("切换镜像失败: {e}")),
            }
            return;
        }
        // 镜像源视图：应用预设
        let tool = self.current_mirror_tool();
        let presets = self.current_presets();
        let Some(preset) = presets.get(self.preset_idx) else { return };
        match self.manager.apply_registry(&tool, &preset.name) {
            Ok(()) => {
                self.status = format!("{tool} 镜像已切换为 {}", preset.name);
                self.refresh_mirror();
            }
            Err(e) => self.error = Some(format!("镜像切换失败: {e}")),
        }
    }

    /// 某 SDK 当前加速镜像名（None = 官方源）
    fn current_mirror_of(&self, t: &ToolInfo) -> Option<String> {
        for m in t.mirrors.iter().flatten() {
            if m.from == m.to {
                continue;
            }
            if self.mirror_cfg.rules.get(&m.from) == Some(&m.to) {
                return Some(m.name.clone());
            }
        }
        None
    }

    pub(crate) fn render_mirror(&mut self, f: &mut ratatui::Frame<'_>, area: Rect) {
        if self.mirror_view == 1 {
            // 下载加速视图
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(area);
            let sdk_tools = self.mirror_sdk_tools();
            let items: Vec<ListItem> = sdk_tools
                .iter()
                .map(|t| {
                    let mirror = self.current_mirror_of(t);
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{:<12}", t.name), Style::default().fg(Color::Cyan)),
                        Span::styled(mirror.unwrap_or_else(|| "官方源".into()), Style::default().fg(Color::Green)),
                    ]))
                })
                .collect();
            let hl = if self.focus_mirror_tools {
                Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().bg(Color::DarkGray)
            };
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("SDK（声明了加速镜像的才可切换）"))
                .highlight_style(hl)
                .highlight_symbol("> ");
            let mut st = ListState::default();
            st.select(Some(self.mirror_idx));
            f.render_stateful_widget(list, chunks[0], &mut st);
            let tool = self.mirror_sdk_tools().get(self.mirror_idx).map(|t| t.name.clone()).unwrap_or_default();
            self.click_lists.push((ClickTarget::MirrorToolList, chunks[0]));

            let options = self.mirror_options();
            let selected = self.current_mirror_selected();
            let items: Vec<ListItem> = options
                .iter()
                .map(|o| {
                    let cur = if o == "官方源" { selected.is_none() } else { selected.as_deref() == Some(o.as_str()) };
                    let mark = if cur { " ● 当前" } else { "" };
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{:<14}", o), Style::default()),
                        Span::styled(mark, Style::default().fg(Color::Green)),
                    ]))
                })
                .collect();
            let hl = if self.focus_mirror_tools {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
            };
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(format!("下载加速 · {tool}（Enter 切换）")))
                .highlight_style(hl)
                .highlight_symbol("> ");
            let mut st = ListState::default();
            st.select(Some(self.preset_idx));
            f.render_stateful_widget(list, chunks[1], &mut st);
            self.click_lists.push((ClickTarget::MirrorPresetList, chunks[1]));
            return;
        }

        // 镜像源视图
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(area);
        let items: Vec<ListItem> = self
            .mirror_tools
            .iter()
            .map(|t| {
                let st = self.mirror_states.get(t);
                let cur = st.and_then(|s| s.preset_name.clone()).unwrap_or_else(|| "未配置".into());
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:<8}", t), Style::default().fg(Color::Cyan)),
                    Span::styled(cur, Style::default().fg(Color::Green)),
                ]))
            })
            .collect();
        let hl = if self.focus_mirror_tools {
            Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().bg(Color::DarkGray)
        };
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("工具镜像源"))
            .highlight_style(hl)
            .highlight_symbol("> ");
        let mut st = ListState::default();
        st.select(Some(self.mirror_idx));
        f.render_stateful_widget(list, chunks[0], &mut st);
        self.click_lists.push((ClickTarget::MirrorToolList, chunks[0]));

        // 右侧：当前状态 + 预设列表（结构化三列：名称 ≤20 cell | 当前标记 8 cell | URL 吃满剩余）
        const NAME_CELLS: usize = 20;
        const CUR_CELLS: usize = 8;
        let tool = self.current_mirror_tool();
        let state = self.current_state();
        let presets = self.current_presets();
        let items: Vec<ListItem> = presets
            .iter()
            .map(|p| {
                let cur = state.and_then(|s| s.preset_name.as_deref()) == Some(p.name.as_str());
                let name_disp = fmt::pad_cells(&fmt::truncate_cells(&p.name, NAME_CELLS), NAME_CELLS);
                let cur_disp = fmt::pad_cells(if cur { "（当前）" } else { "" }, CUR_CELLS);
                let cur_style = if cur {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let mut spans = vec![
                    Span::styled(name_disp, Style::default().fg(if p.is_official { Color::Green } else { Color::Cyan })),
                    Span::styled(cur_disp, cur_style),
                    Span::raw(" "),
                    Span::raw(p.url.clone()),
                ];
                if p.is_custom {
                    spans.push(Span::styled("（自定义）", Style::default().fg(Color::Yellow)));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        let hl = if self.focus_mirror_tools {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
        };
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(format!("预设 · {tool}（Enter 应用 / a 添加 / d/Del 删除自定义）")))
            .highlight_style(hl)
            .highlight_symbol("> ");
        let mut st = ListState::default();
        st.select(Some(self.preset_idx));
        f.render_stateful_widget(list, chunks[1], &mut st);
        self.click_lists.push((ClickTarget::MirrorPresetList, chunks[1]));
    }
}
