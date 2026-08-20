//! 设置 Tab：代理 / 缓存 TTL / 存储路径 / 插件仓库。

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use envhive_core::config::RegistryAddress;

use super::*;

/// 设置行类型
#[derive(Clone, Copy, PartialEq)]
enum SettingRowKind {
    ProxyEnable,
    ProxyUrl,
    CacheTtl,
    StoragePath,
    RegistryEntry(usize),
}

impl TuiApp {
    /// 设置行总数（固定行 + 插件仓库子行）
    pub(crate) fn settings_rows(&self) -> usize {
        4 + self.settings_entries.len()
    }

    /// 设置行索引 → 行类型
    fn setting_row_kind(&self, idx: usize) -> SettingRowKind {
        if idx == 0 {
            SettingRowKind::ProxyEnable
        } else if idx == 1 {
            SettingRowKind::ProxyUrl
        } else if idx == 2 {
            SettingRowKind::CacheTtl
        } else if idx == 3 {
            SettingRowKind::StoragePath
        } else {
            SettingRowKind::RegistryEntry(idx - 4)
        }
    }

    /// 选中的插件仓库条目索引（选中行落在仓库子行内）
    pub(crate) fn selected_registry_entry(&self) -> Option<usize> {
        match self.setting_row_kind(self.settings_idx) {
            SettingRowKind::RegistryEntry(i) => Some(i),
            _ => None,
        }
    }

    pub(crate) fn proxy_enable(&self) -> bool {
        self.manager.config.lock().unwrap().proxy.enable
    }

    pub(crate) fn toggle_setting_switch(&mut self) {
        match self.setting_row_kind(self.settings_idx) {
            SettingRowKind::ProxyEnable => {
                let next = !self.proxy_enable();
                let url = self.manager.config.lock().unwrap().proxy.url.clone();
                match self.manager.set_proxy(url, next) {
                    Ok(()) => self.status = format!("下载代理已{}", if next { "开启" } else { "关闭" }),
                    Err(e) => self.error = Some(format!("设置代理失败: {e}")),
                }
            }
            _ => {}
        }
        self.refresh_config();
    }

    pub(crate) fn activate_setting(&mut self) {
        match self.setting_row_kind(self.settings_idx) {
            SettingRowKind::ProxyEnable => self.toggle_setting_switch(),
            SettingRowKind::ProxyUrl => {
                let cur = self.manager.config.lock().unwrap().proxy.url.clone().unwrap_or_default();
                self.start_input(InputField::ProxyUrl, "代理地址（如 http://127.0.0.1:7890）", &cur);
            }
            SettingRowKind::CacheTtl => {
                let cur = self.manager.config.lock().unwrap().cache.available_hook_duration.clone();
                self.start_input(InputField::CacheTtl, "缓存有效期（12h / 3600 / -1 / 0）", &cur);
            }
            SettingRowKind::StoragePath => {
                let cur = self.manager.config.lock().unwrap().storage.tool_path.clone();
                self.start_input(InputField::StoragePath, "工具存储路径（修改需重启生效）", &cur);
            }
            _ => {}
        }
    }

    pub(crate) fn edit_setting(&mut self) {
        match self.setting_row_kind(self.settings_idx) {
            SettingRowKind::ProxyUrl => self.activate_setting(),
            SettingRowKind::CacheTtl => self.activate_setting(),
            SettingRowKind::StoragePath => self.activate_setting(),
            _ => self.status = "该行不可编辑（开关用空格，仓库行用 a/d）".into(),
        }
    }

    pub(crate) fn save_registry_entries(&self) -> Result<()> {
        let mut cfg = self.manager.config.lock().unwrap();
        cfg.registry.addresses = self
            .settings_entries
            .iter()
            .map(|e| RegistryAddress::Named(e.clone()))
            .collect();
        cfg.save(&self.manager.paths.config_file)
    }

    pub(crate) fn refresh_config(&mut self) {
        let cfg = self.manager.config.lock().unwrap().clone();
        self.settings_entries = cfg.registry.entries();
        self.mirror_cfg = cfg.download_mirror.clone();
        self.proxy_enabled = cfg.proxy.enable;
        self.proxy_url = cfg.proxy.url.clone();
        self.cache_ttl = cfg.cache.available_hook_duration.clone();
        self.storage_path = cfg.storage.tool_path.clone();
        self.config_writable = cfg.verify_writable().is_ok();
        self.status = "配置已刷新".into();
    }

    pub(crate) fn render_settings(&self, f: &mut ratatui::Frame<'_>, area: Rect) {
        let rows: Vec<ListItem> = (0..self.settings_rows())
            .map(|i| {
                let (label, value, color) = match self.setting_row_kind(i) {
                    SettingRowKind::ProxyEnable => ("下载代理".to_string(), fmt::on_off(self.proxy_enabled), Color::Green),
                    SettingRowKind::ProxyUrl => ("代理地址".to_string(), self.proxy_url.clone().unwrap_or_else(|| "（未设置）".into()), Color::Cyan),
                    SettingRowKind::CacheTtl => ("缓存有效期".to_string(), self.cache_ttl.clone(), Color::Cyan),
                    SettingRowKind::StoragePath => ("工具存储路径".to_string(), self.storage_path.clone(), Color::Cyan),
                    SettingRowKind::RegistryEntry(idx) => {
                        let e = self.settings_entries.get(idx);
                        let value = e.map(|e| format!("{}  {}", e.name, e.url)).unwrap_or_default();
                        ("插件仓库".to_string(), value, Color::Cyan)
                    }
                };
                let mut spans = vec![Span::styled(format!("{:<18}", label), Style::default())];
                if let SettingRowKind::RegistryEntry(_) = self.setting_row_kind(i) {
                    spans.push(Span::styled("  ", Style::default()));
                }
                spans.push(Span::styled(value, Style::default().fg(color)));
                ListItem::new(Line::from(spans))
            })
            .collect();
        let list = List::new(rows)
            .block(Block::default().borders(Borders::ALL).title("设置（空格 开关 / e 编辑 / a 添加仓库 / d/Del 删除仓库 / r 刷新）"))
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD))
            .highlight_symbol("> ");
        let mut st = ListState::default();
        st.select(Some(self.settings_idx));
        f.render_stateful_widget(list, area, &mut st);
    }
}
