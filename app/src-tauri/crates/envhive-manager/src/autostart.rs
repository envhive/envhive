//! 开机自启动（roadmap P2：可选功能，常驻托盘 + 新终端自动注入环境）
//! Windows：`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 写 "EnvHive" = 当前 exe 路径
//! Linux：`~/.config/autostart/envhive.desktop`
//! macOS：`~/Library/LaunchAgents/com.envhive.plist`

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};

/// 当前是否已启用开机自启动
pub fn is_enabled() -> bool {
    #[cfg(windows)]
    {
        let Some(exe) = std::env::current_exe().ok() else { return false };
        let canonical = exe.to_string_lossy();
        let with_arg = format!("{canonical} --autostart");
        match run_value() {
            // 写入的是 "{exe} --autostart"，但为兼容手动仅写 exe 的情况，两种形式都视为已开启
            Ok(Some(v)) => {
                v.eq_ignore_ascii_case(&canonical) || v.eq_ignore_ascii_case(&with_arg)
            }
            _ => false,
        }
    }
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().unwrap_or_default();
        home.join("Library/LaunchAgents/com.envhive.plist").exists()
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        let home = dirs::home_dir().unwrap_or_default();
        home.join(".config/autostart/envhive.desktop").exists()
    }
}

/// 设置 / 取消开机自启动
pub fn set_enabled(enable: bool) -> Result<()> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe()
            .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Io, "无法获取应用路径", e))?;
        set_run_value(&exe.to_string_lossy(), enable)
    }
    #[cfg(target_os = "macos")]
    {
        write_launchagent(enable)
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        write_autostart_desktop(enable)
    }
}

// ---------------------------------------------------------------------------
// Windows：Run 键
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn run_key_path() -> Vec<u16> {
    "Software\\Microsoft\\Windows\\CurrentVersion\\Run".encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn run_value() -> Result<Option<String>> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, REG_SZ,
        REG_VALUE_TYPE,
    };
    let mut hkey: HKEY = std::ptr::null_mut();
    let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, run_key_path().as_ptr(), 0, KEY_QUERY_VALUE, &mut hkey) };
    if rc != ERROR_SUCCESS {
        return Ok(None);
    }
    let name: Vec<u16> = "EnvHive".encode_utf16().chain(std::iter::once(0)).collect();
    let mut buf = vec![0u8; 4096];
    let mut size = 4096u32;
    let mut ty: REG_VALUE_TYPE = 0;
    let rc = unsafe {
        RegQueryValueExW(hkey, name.as_ptr(), std::ptr::null_mut(), &mut ty, buf.as_mut_ptr(), &mut size)
    };
    unsafe { RegCloseKey(hkey) };
    if rc != ERROR_SUCCESS || ty != REG_SZ {
        return Ok(None);
    }
    buf.truncate(size as usize);
    // 只去除末尾「成对」的 0x00（完整 UTF-16 空字符），不能按单个 0x00 字节去除，
    // 否则会吃掉最后一个字符的高字节，并使 buf 变为奇数长度导致 chunks(2) 越界 panic。
    while buf.len() >= 2 && buf[buf.len() - 1] == 0 && buf[buf.len() - 2] == 0 {
        buf.pop();
        buf.pop();
    }
    let s = String::from_utf16_lossy(
        &buf.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<u16>>(),
    );
    Ok(Some(s.trim().to_string()))
}

#[cfg(windows)]
fn set_run_value(exe: &str, enable: bool) -> Result<()> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    let mut hkey: HKEY = std::ptr::null_mut();
    let mut disp = 0u32;
    let rc = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            run_key_path().as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut hkey,
            &mut disp,
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(EnvHiveError::new(EnvHiveErrorKind::Registry, format!("打开 Run 注册表键失败（{rc}）")));
    }
    let name: Vec<u16> = "EnvHive".encode_utf16().chain(std::iter::once(0)).collect();
    let result = if enable {
        let value: Vec<u16> = format!("{exe} --autostart").encode_utf16().chain(std::iter::once(0)).collect();
        let data = unsafe { std::slice::from_raw_parts(value.as_ptr() as *const u8, value.len() * 2) };
        unsafe { RegSetValueExW(hkey, name.as_ptr(), 0, REG_SZ, data.as_ptr(), data.len() as u32) }
    } else {
        let rc = unsafe { RegDeleteValueW(hkey, name.as_ptr()) };
        // 值不存在（2=ERROR_FILE_NOT_FOUND）视为已关闭
        if rc == 2 { 0 } else { rc }
    };
    unsafe { RegCloseKey(hkey) };
    if result == ERROR_SUCCESS {
        tracing::info!("开机自启动已{}", if enable { "开启" } else { "关闭" });
        Ok(())
    } else {
        Err(EnvHiveError::new(EnvHiveErrorKind::Registry, format!("写入 Run 注册表键失败（{result}）")))
    }
}

// ---------------------------------------------------------------------------
// Linux：~/.config/autostart/*.desktop
// ---------------------------------------------------------------------------

#[cfg(all(not(windows), not(target_os = "macos")))]
fn write_autostart_desktop(enable: bool) -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Io, "无法确定主目录"))?;
    let dir = home.join(".config/autostart");
    let file = dir.join("envhive.desktop");
    if !enable {
        let _ = std::fs::remove_file(&file);
        tracing::info!("开机自启动已关闭");
        return Ok(());
    }
    let exe = std::env::current_exe()
        .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Io, "无法获取应用路径", e))?;
    std::fs::create_dir_all(&dir)?;
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=EnvHive\nExec=\"{}\" --autostart\nX-GNOME-Autostart-enabled=true\nComment=EnvHive 常驻托盘\n",
        exe.display()
    );
    std::fs::write(&file, content)?;
    tracing::info!("开机自启动已开启 -> {}", file.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// macOS：LaunchAgent plist
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn write_launchagent(enable: bool) -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| EnvHiveError::new(EnvHiveErrorKind::Io, "无法确定主目录"))?;
    let dir = home.join("Library/LaunchAgents");
    let file = dir.join("com.envhive.plist");
    if !enable {
        let _ = std::fs::remove_file(&file);
        tracing::info!("开机自启动已关闭");
        return Ok(());
    }
    let exe = std::env::current_exe()
        .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Io, "无法获取应用路径", e))?;
    std::fs::create_dir_all(&dir)?;
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key><string>com.envhive</string>\n  <key>ProgramArguments</key>\n  <array>\n    <string>{}</string>\n    <string>--autostart</string>\n  </array>\n  <key>RunAtLoad</key><true/>\n</dict>\n</plist>\n",
        exe.display()
    );
    std::fs::write(&file, content)?;
    tracing::info!("开机自启动已开启 -> {}", file.display());
    Ok(())
}
