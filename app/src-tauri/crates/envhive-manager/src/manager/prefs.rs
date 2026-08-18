//! EnvHiveManager · 偏好设置与项目预设
//! 开机自启动 / 托盘常驻 / 项目预设（列表、保存、删除、会话启动）

use envhive_core::env::Envs;
use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};

use super::EnvHiveManager;

/// 让子进程（cmd / IDE）使用自己 console 的默认句柄，而非 inherit 父进程句柄。
///
/// Tauri 是 Windows GUI 子系统进程：没有关联 console，stdin/stdout/stderr 句柄无效。
/// `Command::spawn()` 默认 `Stdio::inherit()` 会把无效句柄传给子进程，
/// 子进程的 stderr 输出（错误信息）会静默丢失；stdout 会被 cmd 自行修复故表现正常。
///
/// 修复方式：显式传 **NULL handle**（注意：不是 `Stdio::null()`，那是重定向到 NUL 设备丢弃）。
/// CreateProcess 文档：`hStdXxx == NULL` 且设置 `STARTF_USESTDHANDLES` 时，
/// 标准句柄取「子进程所关联 console 的默认句柄」—— 即 stdin=键盘、stdout/stderr=屏幕。
#[cfg(windows)]
fn detach_console_stdio(cmd: &mut std::process::Command) {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use std::process::Stdio;
    unsafe {
        cmd.stdin(Stdio::from(OwnedHandle::from_raw_handle(std::ptr::null_mut())));
        cmd.stdout(Stdio::from(OwnedHandle::from_raw_handle(std::ptr::null_mut())));
        cmd.stderr(Stdio::from(OwnedHandle::from_raw_handle(std::ptr::null_mut())));
    }
}

#[cfg(not(windows))]
fn detach_console_stdio(_cmd: &mut std::process::Command) {}

impl EnvHiveManager {
    // -----------------------------------------------------------------------
    // P2 · 增殖：开机自启动 / 托盘常驻
    // -----------------------------------------------------------------------

    /// 开机自启动实际状态（读注册表 / autostart 文件）
    pub fn autostart_actual(&self) -> bool {
        crate::autostart::is_enabled()
    }

    /// 设置开机自启动（写系统 + 同步 config.yaml）
    pub fn set_autostart(&self, enable: bool) -> Result<()> {
        crate::autostart::set_enabled(enable)?;
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.autostart.enable = enable;
            cfg.save(&self.paths.config_file)?;
        }
        tracing::info!("开机自启动: {}", if enable { "开启" } else { "关闭" });
        Ok(())
    }

    /// 托盘常驻状态（关闭窗口 → 隐藏到托盘，后台继续运行）
    pub fn tray_resident(&self) -> bool {
        self.config.lock().unwrap().autostart.tray_resident
    }

    /// 设置托盘常驻（写 config.yaml；运行期由窗口 close 事件实时读取，即时生效）
    pub fn set_tray_resident(&self, enable: bool) -> Result<()> {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.autostart.tray_resident = enable;
            cfg.save(&self.paths.config_file)?;
        }
        tracing::info!("托盘常驻: {}", if enable { "开启" } else { "关闭" });
        Ok(())
    }

    // -----------------------------------------------------------------------
    // UI v2：项目预设（应用内版本组合）—— 列表 / 保存 / 删除 / 会话启动
    // -----------------------------------------------------------------------

    pub fn list_projects(&self) -> Result<Vec<crate::projects::ProjectPreset>> {
        crate::projects::load(&self.paths)
    }

    pub fn save_project(&self, preset: crate::projects::ProjectPreset) -> Result<()> {
        if !envhive_core::util::safe_component(&preset.name) {
            return Err(EnvHiveError::new(EnvHiveErrorKind::Config, "项目名称非法（不可含路径分隔符）"));
        }
        let mut list = crate::projects::load(&self.paths)?;
        if let Some(existing) = list.iter_mut().find(|p| p.name == preset.name) {
            *existing = preset;
        } else {
            list.push(preset);
        }
        crate::projects::save(&self.paths, &list)
    }

    pub fn delete_project(&self, name: &str) -> Result<()> {
        let mut list = crate::projects::load(&self.paths)?;
        list.retain(|p| p.name != name);
        crate::projects::save(&self.paths, &list)
    }

    /// 按项目预设版本组合构造会话 Envs（PATH 指向预设版本目录 + RootDir 变量 + 全局 [env]）
    fn resolve_preset_envs(&self, preset: &crate::projects::ProjectPreset) -> Result<Envs> {
        let mut envs = Envs::new();
        for item in &preset.versions {
            let Ok(tool) = self.lookup_tool(&item.tool) else { continue };
            let v = envhive_toolkit::tool::version::Version::new(item.version.clone());
            if !tool.is_installed(&self.paths, &v) {
                continue;
            }
            // bin 目录 = 版本目录 + bin_suffix（不依赖 current 链接）
            let version_dir = self.paths.version_dir(&item.tool, &item.version);
            let bin = version_dir.join(tool.desc().bin_suffix().trim_start_matches('/'));
            envs.prepend_path(bin.to_string_lossy().to_string());
            // RootDir 环境变量（如 JAVA_HOME → 版本根目录）
            for (k, kind) in tool.desc().env_vars() {
                if matches!(kind, envhive_toolkit::tool::EnvVarKind::RootDir) {
                    envs.var(*k, version_dir.to_string_lossy().to_string());
                }
            }
        }
        // 全局 env（config.yaml [env]）并入
        for (k, v) in self.config.lock().unwrap().env_global() {
            envs.var(k.clone(), v.clone());
        }
        Ok(envs)
    }

    /// 会话启动：按项目预设注入版本组合环境后 spawn 命令（终端 / IDE / 开发服务器）
    pub fn launch_session(
        &self,
        project_name: &str,
        command: &str,
        args: &[String],
    ) -> Result<()> {
        let presets = crate::projects::load(&self.paths)?;
        let preset = presets.iter().find(|p| p.name == project_name).ok_or_else(|| {
            EnvHiveError::new(EnvHiveErrorKind::Config, format!("项目预设 {project_name} 不存在"))
        })?;
        let envs = self.resolve_preset_envs(preset)?;
        let mut cmd = std::process::Command::new(command);
        cmd.args(args);
        // 用 apply_to_command 注入：set 用 env 覆盖，unset（None）用 env_remove 真正删除。
        envs.apply_to_command(&mut cmd);
        // Windows：Tauri 是 GUI 子系统进程，本身没有有效 console 句柄，
        // 默认 Stdio::inherit() 会把无效句柄传给 cmd.exe，导致其子进程（lua / java 等）
        // 的 stderr 输出丢失（stdout 会被 cmd 自行修复，stderr 不会）。
        // 显式传 NULL handle：CreateProcess 语义 = 子进程使用自己 console 的默认句柄
        // （stdin 键盘输入 / stdout、stderr 屏幕输出），等价于 `2> con`。
        detach_console_stdio(&mut cmd);
        // 工作目录 = 项目目录（存在时）
        let dir = envhive_core::util::expand_tilde(&preset.dir);
        if dir.is_dir() {
            cmd.current_dir(&dir);
        }
        let child = cmd.spawn().map_err(|e| {
            EnvHiveError::with_source(EnvHiveErrorKind::Install, format!("启动 {command} 失败"), e)
        })?;
        tracing::info!("[session] 项目 {project_name} 已启动 {command}（pid={}）", child.id());
        Ok(())
    }
}
