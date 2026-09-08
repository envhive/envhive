//! 通用小工具

use std::path::{Path, PathBuf};

/// 展开 `~` 开头的路径为用户主目录；非 `~` 路径原样返回。
pub fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    } else if let Some(rest) = path.strip_prefix("~/") {
        dirs::home_dir()
            .map(|h| h.join(rest))
            .unwrap_or_else(|| PathBuf::from(path))
    } else if let Some(rest) = path.strip_prefix("~\\") {
        // 修复：Windows 风格 `~\sdks` 此前落入 else 分支，生成名为 `~` 的真实目录
        dirs::home_dir()
            .map(|h| h.join(rest))
            .unwrap_or_else(|| PathBuf::from(path))
    } else {
        PathBuf::from(path)
    }
}

/// 文件名校验：确保 tool / version 名称不包含路径分隔符（防路径穿越）
pub fn safe_component(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(':') // 修复：拦截 `:`（Windows 上 base.join("D:evil") 会丢弃 base 变成 "D:evil"，路径穿越）
        && !name.contains('\0')
}

/// 从 URL 推断压缩格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressKind {
    Zip,
    TarGz,
    TarXz,
    SevenZip, // 暂不支持（P1 引入 sevenz-rust）
    /// 单文件可执行二进制（非归档，如 Lua 预编译静态二进制）：
    /// 下载后直接作为安装内容，不解压。
    Binary,
    Unknown,
}

impl CompressKind {
    pub fn from_url(url: &str) -> Self {
        let lower = url.to_ascii_lowercase();
        // 只看路径末段（忽略域名中的 .com 等干扰，也兼容 ?query 参数）
        let last = lower.rsplit(['/', '?']).next().unwrap_or(&lower);
        if last.ends_with(".zip") {
            CompressKind::Zip
        } else if last.ends_with(".tar.gz") || last.ends_with(".tgz") {
            CompressKind::TarGz
        } else if last.ends_with(".tar.xz") || last.ends_with(".txz") {
            CompressKind::TarXz
        } else if last.ends_with(".7z") {
            CompressKind::SevenZip
        } else if last.ends_with(".exe")
            || last.ends_with(".bin")
            || !last.contains('.')
        {
            // 单文件可执行：无扩展名（如 GitHub 资产 lua54 / lua54-macos-arm64）
            // 或明确 .exe / .bin 后缀
            CompressKind::Binary
        } else {
            CompressKind::Unknown
        }
    }

    pub fn is_supported(&self) -> bool {
        matches!(
            self,
            CompressKind::Zip | CompressKind::TarGz | CompressKind::TarXz | CompressKind::Binary
        )
    }

    /// 是否为单文件可执行二进制（安装走 install_binary，不解压）
    pub fn is_binary(&self) -> bool {
        matches!(self, CompressKind::Binary)
    }
}

/// 将目录下的内容（单层）移动到目标：目标已存在则先删除
pub fn replace_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    if dst.exists() {
        if dst.is_dir() {
            tracing::info!("替换安装目录：删除旧目录 {}", dst.display());
            std::fs::remove_dir_all(dst).map_err(|e| {
                tracing::warn!("删除旧安装目录 {} 失败（文件可能被占用）: {e}", dst.display());
                e
            })?;
        } else {
            std::fs::remove_file(dst).map_err(|e| {
                tracing::warn!("删除旧安装文件 {} 失败: {e}", dst.display());
                e
            })?;
        }
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Windows：解压刚完成立刻 rename 整个目录时，实时防护（Defender）可能仍持有新写入
    // 文件的扫描句柄（未声明 FILE_SHARE_DELETE），MoveFileExW 对整个目录返回
    // ERROR_ACCESS_DENIED（os error 5）—— 正常扫描动作，事件日志无记录。
    // 退避重试等扫描完成（与 verify_install 的 Defender 重试机制一致）。
    const MAX_ATTEMPTS: u32 = 5;
    let mut last_err: Option<std::io::Error> = None;
    for attempt in 0..MAX_ATTEMPTS {
        match std::fs::rename(src, dst) {
            Ok(()) => {
                tracing::info!("原子安装完成: {} → {}", src.display(), dst.display());
                return Ok(());
            }
            Err(e) if matches!(e.raw_os_error(), Some(17) | Some(18)) => {
                // 跨卷 rename 失败（Windows ERROR_NOT_SAME_DEVICE=17 / Unix EXDEV=18）：
                // 回退为 copy + 删除源，避免安装目录与临时目录不同盘时安装必失败。
                tracing::warn!("跨卷 rename 失败（{}），回退 copy + 删除源", e);
                copy_dir(src, dst)?;
                let _ = std::fs::remove_dir_all(src);
                return Ok(());
            }
            Err(e) if e.raw_os_error() == Some(5) && cfg!(windows) && attempt + 1 < MAX_ATTEMPTS => {
                last_err = Some(e);
                let wait_ms = 300u64 * (attempt as u64 + 1);
                tracing::warn!(
                    "rename {} → {} 被拒绝（可能被实时防护扫描锁定），{}ms 后重试",
                    src.display(),
                    dst.display(),
                    wait_ms
                );
                std::thread::sleep(std::time::Duration::from_millis(wait_ms));
            }
            Err(e) => {
                tracing::warn!("rename {} → {} 失败: {e}", src.display(), dst.display());
                return Err(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Other, "rename 安装目录失败")
    }))
}

/// 递归拷贝目录（replace_dir / normalize_windows_pbs_layout 跨卷回退用）
pub fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&path, &target)?;
        } else {
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

/// 拼接命令输出（stdout + stderr），用于安装后验证
pub fn output_text(output: &std::process::Output) -> String {
    let mut s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !err.is_empty() {
        if !s.is_empty() {
            s.push('\n');
        }
        s.push_str(&err);
    }
    s
}

/// Windows：给子进程设置 CREATE_NO_WINDOW，避免 GUI 应用启动 console 子进程时闪控制台窗口。
/// Unix 上为空操作。
#[cfg(windows)]
pub fn hide_console(cmd: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub fn hide_console(_cmd: &mut std::process::Command) {}
