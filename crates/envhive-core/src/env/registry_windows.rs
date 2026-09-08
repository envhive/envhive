//! Windows 用户级注册表环境变量操作（roadmap C，对应 vfox registry_windows.go）
//! 目标：`HKCU\Environment`（用户级，无需管理员）。
//! - PATH 用 REG_EXPAND_SZ 写回，保持 `%VAR%` 展开形式
//! - 修改后 SendMessageTimeoutW(WM_SETTINGCHANGE) 广播，新进程立即感知

#![cfg(windows)]

use std::path::Path;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG};

use crate::error::{EnvHiveError, EnvHiveErrorKind, Result};

const ENV_KEY: HKEY = HKEY_CURRENT_USER;
const ENV_SUBKEY: &str = "Environment";
const WM_SETTINGCHANGE: u32 = 0x001A;
const MAX_PATH_BUF: u32 = 32767;

/// 读用户级环境变量（REG_EXPAND_SZ / REG_SZ 均支持，返回原始字符串）
pub fn get_user_env_var(name: &str) -> Result<Option<String>> {
    let name_wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let key_wide: Vec<u16> = ENV_SUBKEY.encode_utf16().chain(std::iter::once(0)).collect();

    let mut hkey: HKEY = std::ptr::null_mut();
    let rc = unsafe { RegOpenKeyExW(ENV_KEY, key_wide.as_ptr(), 0, KEY_QUERY_VALUE, &mut hkey) };
    if rc != ERROR_SUCCESS {
        return Err(EnvHiveError::new(EnvHiveErrorKind::Link, format!("打开注册表键失败（错误码 {rc}）")));
    }

    let mut buf = vec![0u8; MAX_PATH_BUF as usize];
    let mut size = MAX_PATH_BUF;
    let mut ty: REG_VALUE_TYPE = 0;
    let rc = unsafe {
        RegQueryValueExW(
            hkey,
            name_wide.as_ptr(),
            std::ptr::null_mut(),
            &mut ty,
            buf.as_mut_ptr(),
            &mut size,
        )
    };
    unsafe { RegCloseKey(hkey) };

    if rc == 2 {
        // ERROR_FILE_NOT_FOUND
        return Ok(None);
    }
    if rc != ERROR_SUCCESS {
        return Err(EnvHiveError::new(EnvHiveErrorKind::Link, format!("读取环境变量 {name} 失败（错误码 {rc}）")));
    }
    buf.truncate(size as usize);
    // 注册表 REG_SZ / REG_EXPAND_SZ 的 size 包含尾随的 UTF-16 空终止符（完整的 0x0000，即两字节 0x00）。
    // 注意：绝不能按「单个 0x00 字节」去除——每个 ASCII 字符的高字节就是 0x00，那样会吃掉最后一个字符，
    // 导致 PATH 被截断写回、autostart/sysproxy 的 chunks(2) 因奇数长度越界 panic。
    // 因此只去除末尾「成对」的 0x00（即完整的 UTF-16 空字符）。
    while buf.len() >= 2 && buf[buf.len() - 1] == 0 && buf[buf.len() - 2] == 0 {
        buf.pop();
        buf.pop();
    }
    // 防御：确保为偶数长度，避免 u16 视图丢字节或越界
    if buf.len() % 2 != 0 {
        buf.pop();
    }
    let s = String::from_utf16_lossy(bytemuck_slice_u16(&buf));
    Ok(Some(s))
}

/// 写用户级环境变量；`expand=true` 用 REG_EXPAND_SZ（保持 %VAR% 语义）
pub fn set_user_env_var(name: &str, value: &str, expand: bool) -> Result<()> {
    let name_wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let key_wide: Vec<u16> = ENV_SUBKEY.encode_utf16().chain(std::iter::once(0)).collect();
    let value_wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let data = unsafe {
        std::slice::from_raw_parts(value_wide.as_ptr() as *const u8, value_wide.len() * 2)
    };

    let mut hkey: HKEY = std::ptr::null_mut();
    let rc = unsafe {
        RegOpenKeyExW(
            ENV_KEY,
            key_wide.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(EnvHiveError::new(EnvHiveErrorKind::Link, format!("打开注册表键失败（错误码 {rc}）")));
    }
    let ty: REG_VALUE_TYPE = if expand { REG_EXPAND_SZ } else { REG_SZ };
    let rc = unsafe { RegSetValueExW(hkey, name_wide.as_ptr(), 0, ty, data.as_ptr(), data.len() as u32) };
    unsafe { RegCloseKey(hkey) };
    if rc != ERROR_SUCCESS {
        return Err(EnvHiveError::new(EnvHiveErrorKind::Link, format!("写入环境变量 {name} 失败（错误码 {rc}）")));
    }
    Ok(())
}

/// 删除用户级环境变量（RegDeleteValueW 真删，非写入空串）；
/// 键不存在（ERROR_FILE_NOT_FOUND）视为成功。
pub fn delete_user_env_var(name: &str) -> Result<()> {
    let name_wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let key_wide: Vec<u16> = ENV_SUBKEY.encode_utf16().chain(std::iter::once(0)).collect();

    let mut hkey: HKEY = std::ptr::null_mut();
    let rc = unsafe { RegOpenKeyExW(ENV_KEY, key_wide.as_ptr(), 0, KEY_SET_VALUE, &mut hkey) };
    if rc != ERROR_SUCCESS {
        return Err(EnvHiveError::new(EnvHiveErrorKind::Link, format!("打开注册表键失败（错误码 {rc}）")));
    }
    let rc = unsafe { RegDeleteValueW(hkey, name_wide.as_ptr()) };
    unsafe { RegCloseKey(hkey) };
    if rc != ERROR_SUCCESS && rc != 2 {
        return Err(EnvHiveError::new(EnvHiveErrorKind::Link, format!("删除环境变量 {name} 失败（错误码 {rc}）")));
    }
    Ok(())
}

/// 更新用户 PATH：
/// - 移除所有以 `remove_prefix`（大小写不敏感）开头的条目
/// - 头部插入 `add` 中的新条目（大小写不敏感去重）
/// - 保持 REG_EXPAND_SZ 类型写回
pub fn update_user_path(add: &[String], remove_prefix: &str) -> Result<()> {
    let current = get_user_env_var("PATH")?.unwrap_or_default();
    let sep = ';';
    let norm = |p: &str| p.trim().trim_end_matches(['\\', '/']).to_ascii_lowercase();

    let prefix_lower = remove_prefix.trim_end_matches(['\\', '/']).to_ascii_lowercase();

    let mut parts: Vec<String> = current
        .split(sep)
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .collect();

    // 1. 移除 envhive 管理的旧条目
    parts.retain(|p| !norm(p).starts_with(&prefix_lower));

    // 2. 前置插入新条目（去重）
    let mut added: Vec<String> = Vec::new();
    for p in add {
        if p.trim().is_empty() {
            continue;
        }
        let n = norm(p);
        let exists = parts.iter().any(|e| norm(e) == n) || added.iter().any(|e| norm(e) == n);
        if !exists {
            added.push(p.clone());
        }
    }
    added.extend(parts);

    let new_path = added.join(";");
    set_user_env_var("PATH", &new_path, true)?;
    tracing::info!("用户 PATH 已更新（新增 {} 条 envhive 路径）", add.len());
    Ok(())
}

/// 从 PATH 中移除 envhive 管理的条目（卸载/切换时）
pub fn remove_from_user_path(remove_prefix: &str) -> Result<()> {
    let current = get_user_env_var("PATH")?.unwrap_or_default();
    let norm = |p: &str| p.trim().trim_end_matches(['\\', '/']).to_ascii_lowercase();
    let prefix_lower = remove_prefix.trim_end_matches(['\\', '/']).to_ascii_lowercase();
    let parts: Vec<String> = current
        .split(';')
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty() && !norm(s).starts_with(&prefix_lower))
        .collect();
    let new_path = parts.join(";");
    set_user_env_var("PATH", &new_path, true)
}

/// 广播 WM_SETTINGCHANGE，让新进程立即感知环境变量变化（无需重启/注销）
pub fn broadcast_env_change() {
    let lparam_wide: Vec<u16> = "Environment".encode_utf16().chain(std::iter::once(0)).collect();
    let mut result: usize = 0;
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            lparam_wide.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            &mut result,
        );
    }
    tracing::info!("WM_SETTINGCHANGE 已广播");
}

/// 确保 tool 的 bin 目录存在于用户 PATH（供切换版本调用）
pub fn ensure_bin_in_path(bin_dir: &Path, remove_prefix: &str) -> Result<()> {
    let bin_str = bin_dir.to_string_lossy().trim_end_matches('\\').to_string();
    update_user_path(&[bin_str], remove_prefix)
}

// ---------------------------------------------------------------------------

fn bytemuck_slice_u16(buf: &[u8]) -> &[u16] {
    unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u16, buf.len() / 2) }
}
