//! 输入 / 确认模态框。
//!
//! 文本编辑（代理地址、缓存 TTL、存储路径、插件仓库、自定义镜像源）
//! 与危险操作确认均以内嵌模态框完成，不离开 TUI。

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crossterm::event::KeyCode;

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind};

use super::*;

impl TuiApp {
    pub(crate) fn start_input(&mut self, field: InputField, prompt: &str, default: &str) {
        self.input = Some(InputState {
            field,
            prompt: prompt.to_string(),
            buffer: default.to_string(),
            hint: "Enter 确认 · Esc 取消 · Backspace 删除".into(),
        });
    }

    pub(crate) fn on_input_key(&mut self, code: KeyCode) {
        // 读写 buffer 用局部借用；需要改动 self.input 时先结束借用（Esc 取消 / Enter 提交）
        let action: Option<(InputField, String)> = {
            let input = self.input.as_mut();
            let Some(input) = input else { return };
            match code {
                KeyCode::Char(c) => {
                    input.buffer.push(c);
                    None
                }
                KeyCode::Backspace => {
                    input.buffer.pop();
                    None
                }
                KeyCode::Enter => Some((input.field, input.buffer.trim().to_string())),
                _ => None,
            }
        };
        match action {
            Some((field, value)) => {
                self.input = None;
                self.commit_input(field, value);
            }
            None if code == KeyCode::Esc => self.input = None,
            _ => {}
        }
    }

    /// 输入提交：按字段分流处理；返回要展示的状态消息
    fn commit_input(&mut self, field: InputField, value: String) {
        let result: Result<String> = (|| {
            match field {
                InputField::ProxyUrl => {
                    let url = if value.is_empty() { None } else { Some(value) };
                    self.manager.set_proxy(url, self.proxy_enable())?
                }
                InputField::CacheTtl => {
                    if !fmt::valid_ttl(&value) {
                        return Err(EnvHiveError::new(
                            EnvHiveErrorKind::Config,
                            format!("非法缓存 TTL: {value:?}（示例: 12h / 3600 / -1 / 0）"),
                        ));
                    }
                    let mut cfg = self.manager.config.lock().unwrap();
                    cfg.cache.available_hook_duration = value.clone();
                    cfg.save(&self.manager.paths.config_file)?;
                }
                InputField::StoragePath => {
                    if value.is_empty() {
                        return Err(EnvHiveError::new(
                            EnvHiveErrorKind::Config,
                            "存储路径不能为空",
                        ));
                    }
                    let mut cfg = self.manager.config.lock().unwrap();
                    cfg.storage.tool_path = value.clone();
                    cfg.save(&self.manager.paths.config_file)?;
                }
                InputField::RegistryName => {
                    if value.is_empty() {
                        return Err(EnvHiveError::new(
                            EnvHiveErrorKind::Config,
                            "仓库名不能为空",
                        ));
                    }
                    self.pending_registry_name = Some(value);
                    // 进入第 2 步：输入 manifest 地址
                    let prompt = "添加插件仓库：manifest.json 完整地址";
                    self.input = Some(InputState {
                        field: InputField::RegistryUrl,
                        prompt: prompt.into(),
                        buffer: "".into(),
                        hint: "Enter 确认 · Esc 取消 · Backspace 删除".into(),
                    });
                    return Ok("继续输入仓库 manifest 地址…".into());
                }
                InputField::RegistryUrl => {
                    if value.is_empty() || !(value.starts_with("http://") || value.starts_with("https://")) {
                        return Err(EnvHiveError::new(
                            EnvHiveErrorKind::Config,
                            "仓库地址需以 http:// 或 https:// 开头",
                        ));
                    }
                    let name = self.pending_registry_name.take().unwrap_or_else(|| "仓库".into());
                    let url = value.trim_end_matches('/').to_string();
                    if self.settings_entries.iter().any(|e| e.url == url) {
                        return Err(EnvHiveError::new(
                            EnvHiveErrorKind::Config,
                            "该仓库地址已存在",
                        ));
                    }
                    self.settings_entries.push(RegistryEntry { name, url });
                    self.save_registry_entries()?;
                }
                InputField::MirrorName => {
                    if value.is_empty() {
                        return Err(EnvHiveError::new(
                            EnvHiveErrorKind::Config,
                            "镜像名称不能为空",
                        ));
                    }
                    // 名称暂存，进入第 2 步输入地址（pending_mirror 已保存工具名）
                    self.pending_mirror_name = Some(value);
                    let tool = self.pending_mirror.clone().unwrap_or_default();
                    let prompt = format!("为 {tool} 添加自定义镜像源：地址");
                    self.input = Some(InputState {
                        field: InputField::MirrorUrl,
                        prompt,
                        buffer: "".into(),
                        hint: "Enter 确认 · Esc 取消 · Backspace 删除".into(),
                    });
                    return Ok("继续输入镜像地址…".into());
                }
                InputField::MirrorUrl => {
                    let tool = self.pending_mirror.clone().unwrap_or_default();
                    let name = self.pending_mirror_name.take().unwrap_or_default();
                    if value.is_empty() || !(value.starts_with("http://") || value.starts_with("https://")) {
                        return Err(EnvHiveError::new(
                            EnvHiveErrorKind::Config,
                            "镜像地址需以 http:// 或 https:// 开头",
                        ));
                    }
                    self.manager.add_custom_registry_preset(&tool, &name, &value)?;
                }
            }
            Ok("✓ 已保存".into())
        })();
        match result {
            Ok(msg) => {
                self.refresh_config();
                self.refresh_mirror();
                self.status = msg;
            }
            Err(e) => self.error = Some(format!("操作失败: {e}")),
        }
    }

    pub(crate) fn on_confirm_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let Some(c) = self.confirm.take() else { return };
                self.exec_confirm(c.action);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.confirm = None;
            }
            _ => {}
        }
    }

    fn exec_confirm(&mut self, action: ConfirmAction) {
        let result: Result<()> = (|| {
            match action {
                ConfirmAction::DeletePlugin(name) => self.manager.delete_plugin(&name)?,
                ConfirmAction::DeleteMirrorPreset { tool, name } => self.manager.remove_custom_registry_preset(&tool, &name)?,
                ConfirmAction::UninstallVersion { tool, version } => self.manager.uninstall_tool(&tool, &version)?,
                ConfirmAction::RemoveRegistryEntry(idx) => {
                    if self.settings_entries.len() <= 1 {
                        return Err(EnvHiveError::new(
                            EnvHiveErrorKind::Config,
                            "至少保留一个插件仓库",
                        ));
                    }
                    if idx < self.settings_entries.len() {
                        self.settings_entries.remove(idx);
                        self.save_registry_entries()?;
                    }
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                // 仓库删除后行数可能减少：钳制选中行，避免高亮越界
                if self.settings_idx >= self.settings_rows() {
                    self.settings_idx = self.settings_rows().saturating_sub(1);
                }
                self.refresh_config();
                self.refresh_mirror();
                self.refresh_tools();
                self.plugins = self.manager.list_plugins();
                self.status = "✓ 操作完成".into();
            }
            Err(e) => self.error = Some(format!("操作失败: {e}")),
        }
    }

    /// 输入模态框（居中覆盖）
    pub(crate) fn render_input(&self, f: &mut ratatui::Frame<'_>, area: Rect) {
        let Some(input) = &self.input else { return };
        let w = area.width.min(90).saturating_sub(4).max(30);
        let h = 7;
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;
        let rect = Rect::new(x, y, w, h);
        f.render_widget(Clear, rect);
        let lines = vec![
            Line::from(Span::styled(&input.prompt, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from(Span::styled(format!("> {}", input.buffer), Style::default().fg(Color::White))),
            Line::from(""),
            Line::from(Span::styled(&input.hint, Style::default().fg(Color::DarkGray))),
        ];
        let para = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("输入"));
        f.render_widget(para, rect);
    }

    /// 确认模态框（居中覆盖）
    pub(crate) fn render_confirm(&self, f: &mut ratatui::Frame<'_>, area: Rect) {
        let Some(c) = &self.confirm else { return };
        let w = area.width.min(80).saturating_sub(4).max(30);
        let h = 5;
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;
        let rect = Rect::new(x, y, w, h);
        f.render_widget(Clear, rect);
        let lines = vec![
            Line::from(Span::styled(&c.prompt, Style::default().fg(Color::Yellow))),
            Line::from(""),
            Line::from(Span::styled("确认?  y 确认 · n / Esc 取消", Style::default().fg(Color::DarkGray))),
        ];
        let para = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("确认"));
        f.render_widget(para, rect);
    }
}
