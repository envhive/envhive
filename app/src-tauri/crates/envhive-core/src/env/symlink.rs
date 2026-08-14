//! 跨平台链接抽象（roadmap C：Unix symlink / Windows junction 统一接口）
//! Windows 用 junction（reparse point）而非符号链接：创建无需管理员权限，避免 UAC 弹窗。

use std::path::{Path, PathBuf};

use crate::error::{EnvHiveError, EnvHiveErrorKind, Result};

/// 创建链接：`link -> target`（target 为真实目录）。父目录需已存在。
pub fn create_link(target: &Path, link: &Path) -> Result<()> {
    // 清理已存在的链接/文件（Windows junction 的 is_symlink() 恒为 true，须走 remove_link 而非 remove_file）
    if let Ok(md) = link.symlink_metadata() {
        if md.file_type().is_symlink() || md.is_dir() {
            remove_link(link)?;
        } else {
            let _ = std::fs::remove_file(link);
        }
    }
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).map_err(|e| {
            EnvHiveError::with_source(
                EnvHiveErrorKind::Link,
                format!("创建符号链接 {} -> {} 失败", link.display(), target.display()),
                e,
            )
        })
    }

    #[cfg(windows)]
    {
        create_junction(target, link)
    }

    #[cfg(not(any(unix, windows)))]
    {
        Err(EnvHiveError::new(EnvHiveErrorKind::Unsupported, "当前平台不支持目录链接"))
    }
}

/// 删除链接（junction 是目录 reparse point，remove_dir 即可，不会递归删除目标内容）
pub fn remove_link(link: &Path) -> Result<()> {
    let md = match link.symlink_metadata() {
        Ok(md) => md,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(EnvHiveError::from(e)),
    };
    // 注意：Windows 上 Rust 的 FileType::is_dir() == !is_symlink() && 有 DIRECTORY 属性。
    // junction（MOUNT_POINT reparse）带 REPARSE_POINT 位 → is_symlink() 恒为 true → is_dir() 恒为 false。
    // 因此必须显式判断 is_symlink()，否则会把目录型链接误判为文件走 remove_file → os error 5！
    let is_dir_like = md.is_dir() || md.file_type().is_symlink();
    if is_dir_like {
        #[cfg(windows)]
        {
            // Windows：remove_dir 对 junction / symlink-to-dir 均只删除链接本身，不跟随目标
            remove_dir_with_retry(link)
        }
        #[cfg(unix)]
        {
            if md.file_type().is_symlink() {
                std::fs::remove_file(link).map_err(EnvHiveError::from)
            } else {
                Err(EnvHiveError::new(
                    EnvHiveErrorKind::Link,
                    format!("{} 是真实目录而非链接，拒绝删除", link.display()),
                ))
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(EnvHiveError::new(EnvHiveErrorKind::Unsupported, "当前平台不支持"))
        }
    } else {
        std::fs::remove_file(link).map_err(EnvHiveError::from)
    }
}

/// Windows：删除目录（junction）时重试，规避瞬时占用（ERROR_ACCESS_DENIED / os error 5）。
/// 常见占用源：杀毒软件实时扫描、终端/IDE 的工作目录在该目录内、资源管理器打开着它。
/// 这类占用通常毫秒级释放，重试 2~3 次即可成功；仍失败则走 FSCTL_DELETE_REPARSE_POINT
/// 兜底（剥除 reparse 点后再删，规避部分系统对 remove_dir(reparse point) 的拒绝行为），
/// 最终仍失败时给出可操作的提示。
#[cfg(windows)]
fn remove_dir_with_retry(link: &Path) -> Result<()> {
    const MAX_ATTEMPTS: u32 = 4;
    const RETRY_DELAY_MS: u64 = 250;

    for attempt in 1..=MAX_ATTEMPTS {
        match std::fs::remove_dir(link) {
            Ok(()) => return Ok(()),
            Err(e) => {
                tracing::warn!("删除链接 {} 第 {attempt} 次失败（{e}），稍后重试", link.display());
                std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
            }
        }
    }

    // 兜底：剥除 reparse 点后删除（FSCTL_DELETE_REPARSE_POINT）
    tracing::warn!("删除链接 {} 重试仍失败，尝试剥除 reparse 点", link.display());
    if let Err(e) = delete_junction_via_reparse(link) {
        return Err(EnvHiveError::with_source(
            EnvHiveErrorKind::Link,
            format!(
                "移除链接 {} 失败：目录可能被占用（终端/IDE 工作目录、资源管理器或杀毒软件）。\
                 请关闭可能占用它的程序后重试",
                link.display()
            ),
            e.into_source(),
        ));
    }
    Ok(())
}

/// Windows 兜底删除 junction：先发 FSCTL_DELETE_REPARSE_POINT 剥除 reparse 点，
/// 目录变为普通空目录后再 remove_dir。不跟随目标，不会误删真实版本目录。
#[cfg(windows)]
fn delete_junction_via_reparse(link: &Path) -> std::result::Result<(), DeleteJunctionError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::FSCTL_DELETE_REPARSE_POINT;

    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;

    // REPARSE_DATA_BUFFER：仅 ReparseTag 参与（offset 0，len 0）
    let mut buf = [0u8; 16];
    buf[0..4].copy_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());

    let link_wide: Vec<u16> = link.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let handle = unsafe {
        CreateFileW(
            link_wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        return Err(DeleteJunctionError::Open(err));
    }

    let mut returned: u32 = 0;
    let ok = unsafe {
        DeviceIoControl(
            handle,
            FSCTL_DELETE_REPARSE_POINT,
            buf.as_ptr() as *const std::ffi::c_void,
            buf.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    let last_error = unsafe { windows_sys::Win32::Foundation::GetLastError() };
    unsafe { CloseHandle(handle) };

    if ok == 0 {
        return Err(DeleteJunctionError::Ioctl(last_error));
    }
    // 剥除 reparse 点后，目录已为普通空目录
    std::fs::remove_dir(link).map_err(DeleteJunctionError::Remove)
}

/// delete_junction_via_reparse 的错误包装（内部用，统一转 EnvHiveError 时给出源错误）
#[cfg(windows)]
enum DeleteJunctionError {
    Open(u32),
    Ioctl(u32),
    Remove(std::io::Error),
}

#[cfg(windows)]
impl DeleteJunctionError {
    fn into_source(self) -> std::io::Error {
        match self {
            DeleteJunctionError::Remove(e) => e,
            DeleteJunctionError::Open(code) => std::io::Error::from_raw_os_error(code as i32),
            DeleteJunctionError::Ioctl(code) => std::io::Error::from_raw_os_error(code as i32),
        }
    }
}

/// 链接是否存在（不跟随目标）
pub fn link_exists(link: &Path) -> bool {
    link.symlink_metadata().is_ok()
}

/// 读取链接目标
pub fn read_link(link: &Path) -> Result<PathBuf> {
    std::fs::read_link(link).map_err(EnvHiveError::from)
}

// ---------------------------------------------------------------------------
// Windows junction 实现（FSCTL_SET_REPARSE_POINT，免管理员权限）
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn create_junction(target: &Path, link: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;

    let target_abs = if target.is_absolute() {
        target.to_path_buf()
    } else {
        std::env::current_dir()?.join(target)
    };
    let target_str = target_abs.to_string_lossy().replace('/', "\\");
    // 去掉末尾反斜杠（substitute name 规范要求）
    let target_str = target_str.trim_end_matches('\\').to_string();
    let subst = format!("\\??\\{target_str}");

    // REPARSE_MOUNTPOINT_DATA_BUFFER 规范：SubstituteName / PrintName 均为
    // NUL 结尾的宽字符串，且长度（subst_bytes / print_bytes）须包含结尾 NUL。
    // 缺失 NUL 时部分 Windows 版本上 FSCTL_SET_REPARSE_POINT 返回
    // ERROR_INVALID_PARAMETER，导致每次切换都回退到 mklink（闪 cmd 窗口）。
    let mut subst_wide: Vec<u16> = subst.encode_utf16().collect();
    subst_wide.push(0);
    let mut print_wide: Vec<u16> = target_str.encode_utf16().collect();
    print_wide.push(0);
    let subst_bytes = (subst_wide.len() * 2) as u16;
    let print_bytes = (print_wide.len() * 2) as u16;

    // REPARSE_MOUNTPOINT_DATA_BUFFER：
    // ReparseTag(4) + ReparseDataLength(2) + Reserved(2) + SubstituteNameOffset(2)
    //   + SubstituteNameLength(2) + PrintNameOffset(2) + PrintNameLength(2) + PathBuffer
    // Windows 要求整个 buffer（含 ReparseTag/ReparseDataLength 的 8 字节头）按 8 字节对齐，
    // 即 ReparseDataLength 本身须为 8 的倍数，否则 DeviceIoControl 返回 ERROR_INVALID_PARAMETER。
    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
    const HEADER: u32 = 8; // ReparseTag + ReparseDataLength
    let body = 8 + u32::from(subst_bytes) + u32::from(print_bytes); // offset/len 字段 + 数据
    let data_len = (body + 7) / 8 * 8; // ReparseDataLength 对齐到 8 的倍数
    let mut buf: Vec<u8> = Vec::with_capacity((HEADER + data_len) as usize);
    buf.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes()); // ReparseTag
    buf.extend_from_slice(&(data_len as u16).to_le_bytes()); // ReparseDataLength
    buf.extend_from_slice(&0u16.to_le_bytes()); // Reserved
    buf.extend_from_slice(&0u16.to_le_bytes()); // SubstituteNameOffset
    buf.extend_from_slice(&subst_bytes.to_le_bytes()); // SubstituteNameLength
    buf.extend_from_slice(&subst_bytes.to_le_bytes()); // PrintNameOffset（紧跟 SubstituteName 之后）
    buf.extend_from_slice(&print_bytes.to_le_bytes()); // PrintNameLength
    for u in &subst_wide {
        buf.extend_from_slice(&u.to_le_bytes());
    }
    for u in &print_wide {
        buf.extend_from_slice(&u.to_le_bytes());
    }
    // 对齐填充（PathBuffer 尾部补 0，保持 8 字节对齐）
    while (buf.len() as u32) < HEADER + data_len {
        buf.push(0);
    }

    // link 目录必须先存在（若后续失败，需清理该残留目录，避免留下"空 current"假象）
    std::fs::create_dir_all(link)?;

    let link_wide: Vec<u16> = link.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let handle = unsafe {
        CreateFileW(
            link_wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        tracing::error!("创建 junction 失败：无法打开链接目录 {}（错误码 {err}）", link.display());
        // 回滚：目录是本函数刚建的，删除避免残留空目录
        let _ = std::fs::remove_dir(link);
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::Link,
            format!("打开链接目录 {} 失败（错误码 {err}）", link.display()),
        ));
    }

    let mut returned: u32 = 0;
    let ok = unsafe {
        DeviceIoControl(
            handle,
            FSCTL_SET_REPARSE_POINT,
            buf.as_ptr() as *const std::ffi::c_void,
            buf.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    let last_error = unsafe { windows_sys::Win32::Foundation::GetLastError() };
    unsafe { CloseHandle(handle) };

    if ok == 0 {
        // 已存在同目标 junction 时可能报 ERROR_ALREADY_EXISTS，视为成功（幂等）
        if last_error == ERROR_ALREADY_EXISTS && read_link(link).map(|p| p == target_abs).unwrap_or(false) {
            return Ok(());
        }
        tracing::error!("DeviceIoControl(FSCTL_SET_REPARSE_POINT) 失败：{} -> {}（错误码 {last_error}）", link.display(), target_str);
        // 回滚：删除残留的空目录（当前目录尚无 reparse point）
        let _ = std::fs::remove_dir(link);
        // 回退方案：调用系统 mklink /J（与资源管理器/PowerShell 同源，已验证可靠）
        return fallback_mklink_junction(link, &target_str);
    }
    Ok(())
}

/// 回退方案：通过 `cmd /c mklink /J <link> <target>` 创建 junction。
/// 系统自带命令，与 Explorer / PowerShell New-Item -ItemType Junction 同源，兼容性最好。
#[cfg(windows)]
fn fallback_mklink_junction(link: &Path, target_str: &str) -> Result<()> {
    use std::process::Command;
    let mut cmd = Command::new("cmd");
    cmd.args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target_str);
    // 隐藏 cmd 控制台窗口（CREATE_NO_WINDOW）：Tauri 为 GUI 进程，
    // 直接 spawn cmd 会闪出一个黑框窗口。
    crate::util::hide_console(&mut cmd);
    let out = cmd.output();
    match out {
        Ok(o) if o.status.success() => {
            tracing::info!("mklink /J 回退成功：{} -> {}", link.display(), target_str);
            Ok(())
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            let stdout = String::from_utf8_lossy(&o.stdout);
            let msg = if stderr.trim().is_empty() { stdout } else { stderr };
            let err_msg = msg.trim().to_string();
            let err_msg = if err_msg.is_empty() { format!("exit {}", o.status) } else { err_msg };
            // mklink 失败时可能已创建目录，清理避免残留
            let _ = std::fs::remove_dir(link);
            Err(EnvHiveError::new(
                EnvHiveErrorKind::Link,
                format!("创建 junction {} -> {} 失败（mklink：{err_msg}）", link.display(), target_str),
            ))
        }
        Err(e) => {
            let _ = std::fs::remove_dir(link);
            Err(EnvHiveError::with_source(
                EnvHiveErrorKind::Link,
                format!("创建 junction {} -> {} 失败（无法调用 mklink）", link.display(), target_str),
                e,
            ))
        }
    }
}
