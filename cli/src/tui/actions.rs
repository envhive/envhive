//! 键盘事件分发与 Tab 内导航。
//!
//! 顶层按键（1-7 切 Tab、q 退出、方向键、Enter/空格/r/a/d/m/o/u/i/x/X/e/c 等）
//! 按当前 Tab / 子视图分派到对应页面的动作方法；Tab 键负责子视图焦点切换。

use crossterm::event::KeyCode;

use super::*;

impl TuiApp {
    /// 切换到指定 Tab（键盘 1-7 与鼠标点击共用；相同 Tab 不重复触发清屏）
    pub(crate) fn switch_tab(&mut self, t: usize) {
        if t < TAB_TITLES.len() && t != self.tab {
            self.tab = t;
            self.pending_clear = true;
        }
    }

    pub(crate) fn on_key(&mut self, code: KeyCode) {
        // 确认模态优先
        if self.confirm.is_some() {
            self.on_confirm_key(code);
            return;
        }
        // 输入模态
        if self.input.is_some() {
            self.on_input_key(code);
            return;
        }
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.quitting = true,
            KeyCode::Char('1') => self.switch_tab(0),
            KeyCode::Char('2') => self.switch_tab(1),
            KeyCode::Char('3') => self.switch_tab(2),
            KeyCode::Char('4') => self.switch_tab(3),
            KeyCode::Char('5') => self.switch_tab(4),
            KeyCode::Char('6') => self.switch_tab(5),
            KeyCode::Char('7') => self.switch_tab(6),
            KeyCode::Tab => self.on_tab(),
            KeyCode::Up => self.move_sel(-1),
            KeyCode::Down => self.move_sel(1),
            KeyCode::Enter => self.on_enter(),
            KeyCode::Char(' ') => self.on_space(),
            KeyCode::Char('r') => self.on_refresh(),
            KeyCode::Char('a') => self.on_add(),
            // 工具页小写 d 也用于切换发行商（无发行商维度的工具会提示；其他 Tab 仍是删除语义）
            KeyCode::Char('d') if self.tab == 0 => self.cycle_distribution(),
            KeyCode::Char('d') | KeyCode::Delete => self.on_delete(),
            KeyCode::Char('m') => {
                if self.tab == 1 {
                    self.plugin_view = 1 - self.plugin_view;
                    self.plugin_idx = 0;
                    self.status = if self.plugin_view == 1 { "插件市场（Enter 安装 / r 刷新）" } else { "本地插件（空格 启停 / d/Del 删除 / o 打开目录）" }.into();
                } else if self.tab == 3 {
                    self.mirror_view = 1 - self.mirror_view;
                    self.preset_idx = 0;
                    self.status = if self.mirror_view == 1 { "下载加速镜像（Enter 切换）" } else { "镜像源（Enter 应用预设 / a 添加 / d/Del 删除自定义）" }.into();
                }
            }
            KeyCode::Char('o') => {
                if self.tab == 1 && self.plugin_view == 0 {
                    self.open_plugin_dir();
                }
            }
            KeyCode::Char('u') => {
                if self.tab == 4 {
                    self.confirm_uninstall_version();
                }
            }
            KeyCode::Char('i') => {
                if self.tab == 1 && self.plugin_view == 1 {
                    self.install_remote_plugin();
                }
            }
            KeyCode::Char('x') => {
                if self.tab == 2 {
                    let n = self.queue.clear_finished();
                    self.status = format!("已清空 {n} 个已完成任务");
                }
            }
            KeyCode::Char('e') => {
                if self.tab == 5 {
                    self.edit_setting();
                }
            }
            KeyCode::Char('c') => {
                if self.tab == 2 {
                    self.cancel_task();
                }
            }
            KeyCode::Char('D') => {
                if self.tab == 0 {
                    self.cycle_distribution();
                }
            }
            _ => {}
        }
    }

    /// Tab：当前 Tab 内子视图/焦点切换；无子视图的 Tab 循环到下一个
    pub(crate) fn on_tab(&mut self) {
        match self.tab {
            0 => {
                self.focus_versions = !self.focus_versions;
                self.status = if self.focus_versions { "版本列表" } else { "工具列表" }.into();
            }
            1 => {
                self.plugin_view = 1 - self.plugin_view;
                self.plugin_idx = 0;
                self.status = if self.plugin_view == 1 { "插件市场（Enter 安装 / r 刷新）" } else { "本地插件（空格 启停 / d/Del 删除 / o 打开目录）" }.into();
            }
            3 => {
                self.focus_mirror_tools = !self.focus_mirror_tools;
                self.preset_idx = 0;
            }
            4 => self.focus_stats_versions = !self.focus_stats_versions,
            _ => {
                self.tab = (self.tab + 1) % TAB_TITLES.len();
                self.pending_clear = true;
            }
        }
    }

    pub(crate) fn move_sel(&mut self, delta: isize) {
        match self.tab {
                0 => {
                    if self.focus_versions {
                        self.ver_idx = fmt::shift(self.ver_idx, delta, self.versions.len());
                    } else {
                        self.tool_idx = fmt::shift(self.tool_idx, delta, self.tools.len());
                        if self.tools.len() > self.tool_idx {
                            self.versions = TuiApp::versions_for(Some(&self.tools[self.tool_idx]), &[]);
                        }
                        self.reset_distribution();
                    }
                }
            1 => {
                if self.plugin_view == 1 {
                    self.plugin_idx = fmt::shift(self.plugin_idx, delta, self.remote_plugins.len());
                } else {
                    self.plugin_idx = fmt::shift(self.plugin_idx, delta, self.plugins.len());
                }
            }
            2 => self.task_idx = fmt::shift(self.task_idx, delta, self.tasks.len()),
            3 => {
                if self.mirror_view == 1 {
                    if self.focus_mirror_tools {
                        self.mirror_idx = fmt::shift(self.mirror_idx, delta, self.mirror_sdk_tools().len());
                        self.preset_idx = 0;
                    } else {
                        self.preset_idx = fmt::shift(self.preset_idx, delta, self.mirror_options().len());
                    }
                } else if self.focus_mirror_tools {
                    self.mirror_idx = fmt::shift(self.mirror_idx, delta, self.mirror_tools.len());
                    self.preset_idx = 0;
                } else {
                    self.preset_idx = fmt::shift(self.preset_idx, delta, self.current_presets().len());
                }
            }
            4 => {
                if self.focus_stats_versions {
                    self.stats_ver_idx = fmt::shift(self.stats_ver_idx, delta, self.stats_versions().len());
                } else {
                    self.stats_idx = fmt::shift(self.stats_idx, delta, self.stats_tools().len());
                    self.stats_ver_idx = 0;
                }
            }
            5 => {
                let total = self.settings_rows();
                self.settings_idx = fmt::shift(self.settings_idx, delta, total);
            }
            _ => {}
        }
    }

    pub(crate) fn on_enter(&mut self) {
        match self.tab {
            0 => {
                if self.focus_versions {
                    self.act_on_version();
                } else {
                    self.fetch_versions();
                }
            }
            1 => {
                if self.plugin_view == 1 {
                    self.install_remote_plugin();
                } else {
                    self.toggle_plugin();
                }
            }
            2 => self.cancel_task(),
            3 => self.activate_mirror(),
            4 => self.confirm_uninstall_version(),
            5 => self.activate_setting(),
            _ => {}
        }
    }

    pub(crate) fn on_space(&mut self) {
        match self.tab {
            1 => {
                if self.plugin_view == 0 {
                    self.toggle_plugin();
                } else {
                    self.install_remote_plugin();
                }
            }
            3 => self.activate_mirror(),
            5 => self.toggle_setting_switch(),
            _ => {}
        }
    }

    pub(crate) fn on_refresh(&mut self) {
        match self.tab {
            0 => self.refresh_tools(),
            1 => {
                if self.plugin_view == 1 {
                    self.fetch_remote_plugins();
                } else {
                    self.plugins = self.manager.list_plugins();
                    self.status = format!("插件列表已刷新（{} 个）", self.plugins.len());
                }
            }
            3 => self.refresh_mirror(),
            4 => self.refresh_stats(),
            5 => self.refresh_config(),
            _ => {}
        }
    }

    pub(crate) fn on_add(&mut self) {
        match self.tab {
            3 if self.mirror_view == 0 => {
                let tool = self.current_mirror_tool();
                self.pending_mirror = Some(tool.clone());
                self.start_input(InputField::MirrorName, &format!("为 {tool} 添加自定义镜像源：名称"), "");
            }
            5 => {
                self.pending_registry_name = None;
                self.start_input(InputField::RegistryName, "添加插件仓库：仓库名（如 官方gitee）", "");
            }
            _ => {}
        }
    }

    pub(crate) fn on_delete(&mut self) {
        match self.tab {
            1 if self.plugin_view == 0 => {
                let Some(p) = self.plugins.get(self.plugin_idx) else { return };
                self.confirm = Some(ConfirmState {
                    prompt: format!("删除插件 {}（仅删定义，已装版本保留）？", p.display),
                    action: ConfirmAction::DeletePlugin(p.name.clone()),
                });
            }
            3 if self.mirror_view == 0 => {
                if let Some(p) = self.current_presets().get(self.preset_idx) {
                    if p.is_custom {
                        let tool = self.current_mirror_tool();
                        self.confirm = Some(ConfirmState {
                            prompt: format!("删除 {tool} 自定义源「{}」？", p.name),
                            action: ConfirmAction::DeleteMirrorPreset { tool, name: p.name.clone() },
                        });
                    } else {
                        self.status = "内置预设不可删除".into();
                    }
                }
            }
            5 => {
                // 删除选中的插件仓库行（仓库行从固定偏移之后开始）
                if let Some(idx) = self.selected_registry_entry() {
                    let name = self.settings_entries[idx].name.clone();
                    self.confirm = Some(ConfirmState {
                        prompt: format!("删除插件仓库「{name}」？"),
                        action: ConfirmAction::RemoveRegistryEntry(idx),
                    });
                }
            }
            _ => {}
        }
    }
}
