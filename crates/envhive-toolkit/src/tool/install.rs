//! 解压与安装（roadmap D：.zip / .tar.gz / .tar.xz → 原子写入，失败清理）
//! 安装流程：解压到临时目录 → 找到内容根（顶层单目录提升）→ 原子 rename → 安装后验证 → 失败自动清理。

use std::io::Read;
use std::path::{Path, PathBuf};

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use envhive_core::pathmeta::PathMeta;
use crate::tool::provider::current_platform;
use crate::tool::ToolDescriptor;
use envhive_core::util::{copy_dir, output_text, replace_dir, CompressKind};

/// 安装上下文
pub struct InstallCtx<'a> {
    pub paths: &'a PathMeta,
    pub desc: &'a dyn ToolDescriptor,
    pub version: &'a str,
    /// v2：选中发行商 key（Lua 插件发行商维度；无则为 None）
    pub distribution: Option<&'a str>,
}

/// root_hint 覆盖适配器（P2：Lua 插件 pre_install 可动态指定解压根目录名）
pub struct RootHintOverride<'a> {
    pub inner: &'a dyn ToolDescriptor,
    pub hint: String,
}

impl<'a> ToolDescriptor for RootHintOverride<'a> {
    fn name(&self) -> &str { self.inner.name() }
    fn display(&self) -> &str { self.inner.display() }
    fn category(&self) -> &str { self.inner.category() }
    fn homepage(&self) -> &str { self.inner.homepage() }
    fn provider(&self) -> crate::tool::provider::ProviderKind { self.inner.provider() }
    fn gh_repo(&self) -> Option<(&str, &str)> { self.inner.gh_repo() }
    fn static_index_url(&self) -> Option<&str> { self.inner.static_index_url() }
    fn url_template(&self) -> Option<&str> { self.inner.url_template() }
    fn platform(&self) -> Option<&crate::tool::PlatformMap> { self.inner.platform() }
    fn verify_bin(&self) -> &str { self.inner.verify_bin() }
    fn verify_arg(&self) -> &str { self.inner.verify_arg() }
    fn bin_suffix(&self) -> &str { self.inner.bin_suffix() }
    fn env_vars(&self) -> &[(&str, crate::tool::EnvVarKind)] { self.inner.env_vars() }
    fn root_hint(&self) -> Option<&str> { Some(&self.hint) }
    fn distributions(&self) -> &[crate::tool::DistributionInfo] { self.inner.distributions() }
    fn default_distribution(&self) -> Option<&str> { self.inner.default_distribution() }
    fn lua_def(&self) -> Option<&crate::lua_plugin::LuaPluginDef> { self.inner.lua_def() }
}

impl<'a> InstallCtx<'a> {
    /// 最终安装目录：`cache/<tool>/v-<version>/`
    pub fn install_dir(&self) -> PathBuf {
        self.paths.version_dir(self.desc.name(), self.version)
    }
}

/// 解压归档到目标目录（zip / tar.gz / tar.xz；.7z 暂不支持 → Unsupported）
pub fn decompress_archive(archive: &Path, dest_dir: &Path, kind: CompressKind) -> Result<()> {
    std::fs::create_dir_all(dest_dir)?;
    match kind {
        CompressKind::Zip => decompress_zip(archive, dest_dir),
        CompressKind::TarGz => {
            let f = std::fs::File::open(archive)?;
            decompress_tar(Box::new(flate2::read::GzDecoder::new(f)), dest_dir)
        }
        CompressKind::TarXz => {
            let f = std::fs::File::open(archive)?;
            decompress_tar(Box::new(xz2::read::XzDecoder::new(f)), dest_dir)
        }
        CompressKind::SevenZip => Err(EnvHiveError::new(
            EnvHiveErrorKind::Unsupported,
            ".7z 格式暂不支持（计划 P1 引入 sevenz-rust）",
        )),
        CompressKind::Binary => Err(EnvHiveError::new(
            EnvHiveErrorKind::Unsupported,
            "单文件二进制无需解压（应走 install_binary）",
        )),
        CompressKind::Unknown => Err(EnvHiveError::new(
            EnvHiveErrorKind::Unsupported,
            format!("无法识别的压缩格式: {}", archive.display()),
        )),
    }
}

fn decompress_zip(archive: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| {
        EnvHiveError::with_source(EnvHiveErrorKind::Decompress, format!("打开 zip 失败: {}", archive.display()), e)
    })?;
    let total = zip.len();
    tracing::info!("解压 zip：{}（{} 条目）→ {}", archive.display(), total, dest_dir.display());

    for i in 0..total {
        // 条目元数据在独立作用域提取（ZipFile 借用 zip，需先 drop 才能再次可变借用）
        let (name, is_dir, out) = {
            let entry = zip.by_index(i).map_err(|e| {
                EnvHiveError::with_source(EnvHiveErrorKind::Decompress, format!("读取 zip 条目 #{i} 失败"), e)
            })?;
            let name = entry.name().to_string();
            let rel = sanitize_rel_path(&name);
            (name, entry.is_dir(), dest_dir.join(rel))
        };
        if is_dir {
            std::fs::create_dir_all(&out).map_err(|e| {
                EnvHiveError::with_source(EnvHiveErrorKind::Io, format!("解压创建目录失败: {}", out.display()), e)
            })?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                EnvHiveError::with_source(EnvHiveErrorKind::Io, format!("解压创建父目录失败: {}", parent.display()), e)
            })?;
        }
        tracing::debug!("[zip #{i}] {} -> {}", name, out.display());
        write_zip_entry(&mut zip, i, &out).map_err(|e| {
            EnvHiveError::with_source(
                EnvHiveErrorKind::Io,
                format!("解压条目失败: {}（zip 内条目 {}）", out.display(), name),
                e,
            )
        })?;
    }
    tracing::info!("解压 zip 完成：{}（{} 条目）", archive.display(), total);
    Ok(())
}

/// 写单个 zip 条目。Windows 实时防护（Defender）可能短暂锁定新写入的 exe
/// （Access denied / os error 5），退避重试（最多 4 次 / ~1.2s）；每次重试重新读取条目。
fn write_zip_entry(
    zip: &mut zip::ZipArchive<std::fs::File>,
    index: usize,
    out: &Path,
) -> std::io::Result<()> {
    const MAX_ATTEMPTS: u32 = 4;
    let mut last_err: Option<std::io::Error> = None;
    for attempt in 0..MAX_ATTEMPTS {
        let mut entry = match zip.by_index(index) {
            Ok(e) => e,
            Err(e) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("重新读取 zip 条目失败: {e}"),
                ))
            }
        };
        match (|| -> std::io::Result<()> {
            let mut f = std::fs::File::create(out)?;
            std::io::copy(&mut entry, &mut f)?;
            Ok(())
        })() {
            Ok(()) => return Ok(()),
            Err(e) if e.raw_os_error() == Some(5) && attempt + 1 < MAX_ATTEMPTS => {
                last_err = Some(e);
                let wait_ms = 200u64 * (attempt as u64 + 1);
                tracing::warn!(
                    "写入 {} 被拒绝（可能被实时防护扫描锁定），{}ms 后重试",
                    out.display(),
                    wait_ms
                );
                std::thread::sleep(std::time::Duration::from_millis(wait_ms));
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Other, "写入 zip 条目失败")
    }))
}

fn decompress_tar(reader: Box<dyn Read + Send>, dest_dir: &Path) -> Result<()> {
    let mut archive = tar::Archive::new(reader);
    let mut entries = archive.entries().map_err(|e| {
        EnvHiveError::with_source(EnvHiveErrorKind::Decompress, "读取 tar 失败", e)
    })?;
    let mut index = 0usize;
    while let Some(entry) = entries.next() {
        let mut entry = entry.map_err(|e| {
            EnvHiveError::with_source(EnvHiveErrorKind::Decompress, "读取 tar 条目失败", e)
        })?;
        let name = entry.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let rel = sanitize_rel_path(&name);
        tracing::debug!("[tar #{index}] {} -> {}", name, rel.display());
        index += 1;
        entry.unpack_in(dest_dir).map_err(|e| {
            EnvHiveError::with_source(
                EnvHiveErrorKind::Decompress,
                format!("解压条目 {} 失败", rel.display()),
                e,
            )
        })?;
    }
    Ok(())
}

/// 清理非法路径组件（防 zip 炸弹路径穿越）
fn sanitize_rel_path(name: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in Path::new(name).components() {
        match comp {
            std::path::Component::Normal(c) => out.push(c),
            _ => {}
        }
    }
    out
}

/// 找到解压内容根：顶层只有一个目录 → 提升该目录为根（如 node-v22.11.0-win-x64/、go/）
fn resolve_extract_root(tmp: &Path, desc: &dyn ToolDescriptor) -> Result<PathBuf> {
    // root_hint 优先（go 解压出 go/ 目录）
    if let Some(hint) = desc.root_hint() {
        let hinted = tmp.join(hint);
        if hinted.is_dir() {
            return Ok(hinted);
        }
    }
    let entries: Vec<_> = std::fs::read_dir(tmp)?
        .filter_map(|e| e.ok())
        .collect();
    if entries.len() == 1 {
        let only = entries[0].path();
        if only.is_dir() {
            return Ok(only);
        }
    }
    Ok(tmp.to_path_buf())
}

/// Windows：将安装目录下所有条目（除 bin/ 外）移入 bin/ 子目录。
/// 用于 python-build-standalone 等构建的可执行文件在根目录的归档布局归一化。
/// 使用 Rust 原生 `std::fs::rename`，无 shell 依赖，无转义问题。
#[cfg(windows)]
pub(crate) fn normalize_windows_pbs_layout(sdk_name: &str, install_dir: &Path) -> Result<()> {
    // 仅对需要布局修正的 工具 生效（当前：python）
    if sdk_name != "python" {
        return Ok(());
    }
    let bin_dir = install_dir.join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    for entry in std::fs::read_dir(install_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path == bin_dir {
            continue; // 排除 bin/ 自身
        }
        let dest = bin_dir.join(entry.file_name());
        // 同卷 rename 是原子的；跨卷回退 copy + 删除
        if let Err(e) = std::fs::rename(&path, &dest) {
            if matches!(e.raw_os_error(), Some(17) | Some(18)) {
                // EXDEV 跨卷 / Win ERROR_NOT_SAME_DEVICE
                if entry.file_type().ok().map_or(false, |ft| ft.is_dir()) {
                    copy_dir(&path, &dest)?;
                } else {
                    std::fs::copy(&path, &dest)?;
                }
                let _ = std::fs::remove_dir_all(&path);
            } else {
                return Err(e.into());
            }
        }
    }
    tracing::info!("[{}] Windows PBS 布局归一化完成（bin/）", sdk_name);
    Ok(())
}

/// 完整安装：解压 → 定位根 → 原子安装 → 验证。失败自动清理。
pub fn install_archive(ctx: &InstallCtx, archive: &Path, kind: CompressKind) -> Result<PathBuf> {
    let install_dir = ctx.install_dir();
    let tmp_dir = ctx.paths.tool_cache_dir(ctx.desc.name()).join(format!(".install-v-{}.tmp", ctx.version));
    tracing::info!(
        "[{}] {} 开始安装：归档 {}（{:?}），目标 {}",
        ctx.desc.name(),
        ctx.version,
        archive.display(),
        kind,
        install_dir.display()
    );

    let result = (|| -> Result<PathBuf> {
        // 1. 清理旧临时目录，解压
        if tmp_dir.exists() {
            tracing::debug!("清理残留临时目录 {}", tmp_dir.display());
            std::fs::remove_dir_all(&tmp_dir).map_err(|e| {
                EnvHiveError::with_source(
                    EnvHiveErrorKind::Io,
                    format!("清理残留临时目录失败（文件可能被占用）: {}", tmp_dir.display()),
                    e,
                )
            })?;
        }
        std::fs::create_dir_all(&tmp_dir)?;
        decompress_archive(archive, &tmp_dir, kind)?;

        // 2. 定位内容根
        let root = resolve_extract_root(&tmp_dir, ctx.desc)?;
        tracing::debug!("解压内容根: {}", root.display());
        if root == tmp_dir {
            tracing::warn!(
                "[{}] {} 解压内容未发现单一顶层目录，直接以临时目录为根",
                ctx.desc.name(),
                ctx.version
            );
        }

        // 3. 原子安装：rename 根 → install_dir（删除旧版本目录）
        tracing::debug!("原子安装: {} → {}", root.display(), install_dir.display());
        replace_dir(&root, &install_dir).map_err(|e| {
            EnvHiveError::with_source(
                EnvHiveErrorKind::Io,
                format!(
                    "安装到 {} 失败（旧版本目录文件可能被占用，如 Tomcat 正在运行或目录被打开）: {}",
                    install_dir.display(),
                    root.display()
                ),
                e,
            )
        })?;
        tracing::info!("[{}] {} 已安装到 {}", ctx.desc.name(), ctx.version, install_dir.display());

        // 3.5a Windows：PBS 构建可执行文件在根目录，统一移入 bin/（Rust 原生，不依赖 PowerShell）
        #[cfg(windows)]
        crate::tool::install::normalize_windows_pbs_layout(ctx.desc.name(), &install_dir)?;

        // 3.5b v2 可选 hook：post_install（解压完成、链接 current 之前；macOS .jdk 整理等）
        if let Some(def) = ctx.desc.lua_def() {
            crate::lua_plugin::hook_post_install(
                def,
                &install_dir.to_string_lossy(),
                ctx.version,
                ctx.distribution,
            );
        }

        // 4. 安装后验证（verify_bin 为空 = 解压即用，跳过运行验证）
        if ctx.desc.verify_bin().is_empty() {
            tracing::info!(
                "[{}] {} 安装成功（verify_bin 为空，跳过运行验证）",
                ctx.desc.name(),
                ctx.version
            );
        } else {
            let ver = verify_install(ctx.desc, &install_dir)?;
            tracing::info!("[{}] {} 安装成功，验证输出: {ver}", ctx.desc.name(), ctx.version);
        }
        Ok(install_dir)
    })();

    // 清理临时目录（成功或失败都清理；失败还需删除可能残留的 install_dir）
    if let Err(e) = std::fs::remove_dir_all(&tmp_dir) {
        tracing::debug!("清理临时目录 {} 失败（可忽略）: {e}", tmp_dir.display());
    }
    if result.is_err() {
        if let Err(e) = std::fs::remove_dir_all(&ctx.install_dir()) {
            tracing::warn!("清理失败安装残留目录失败: {e}");
        }
    }
    result
}

/// 单文件二进制安装（非归档 工具，如 Lua 预编译静态二进制）：
/// 下载的单个可执行文件 → 直接放入安装目录（不解压），统一命名为 工具 名
/// （Windows 追加 .exe），使 verify_bin / env_keys / bin_suffix 跨平台一致
/// （verify_bin 写无扩展名，Windows 安装验证自动探测 .exe）。
/// `extras`：附加文件（已下载，如 Windows 上 exe 同目录所需的 dll），随主文件放入安装目录。
pub fn install_binary(ctx: &InstallCtx, binary: &Path, extras: &[PathBuf]) -> Result<PathBuf> {
    let install_dir = ctx.install_dir();
    let result = (|| -> Result<PathBuf> {
        // 1. 清理旧版本目录（同版本重装场景，避免残留）
        if install_dir.exists() {
            tracing::info!("替换安装目录：删除旧目录 {}", install_dir.display());
            std::fs::remove_dir_all(&install_dir).map_err(|e| {
                EnvHiveError::with_source(
                    EnvHiveErrorKind::Io,
                    format!("清理旧安装目录失败（文件可能被占用）: {}", install_dir.display()),
                    e,
                )
            })?;
        }
        std::fs::create_dir_all(&install_dir)?;

        // 2. 统一可执行名：<tool>[.exe]
        let exe_name = if cfg!(windows) {
            format!("{}.exe", ctx.desc.name())
        } else {
            ctx.desc.name().to_string()
        };
        let dest = install_dir.join(&exe_name);
        tracing::info!(
            "[{}] {} 单文件安装：{} → {}",
            ctx.desc.name(),
            ctx.version,
            binary.display(),
            dest.display()
        );
        move_into_install(binary, &dest, ctx.desc.name(), ctx.version)?;

        // 2.5 附加文件（如 Windows dll）一并移入安装目录
        for extra in extras {
            let extra_dest = install_dir.join(extra.file_name().unwrap_or_default());
            tracing::info!(
                "[{}] {} 附加文件安装：{} → {}",
                ctx.desc.name(),
                ctx.version,
                extra.display(),
                extra_dest.display()
            );
            move_into_install(extra, &extra_dest, ctx.desc.name(), ctx.version)?;
        }

        // 非 Windows：下载文件保存默认无执行权限（0644），补 +x 使 verify / 用户直接可运行
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&dest)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&dest, perms)?;
        }

        // 3. post_install hook（v2：解压/放置完成后、链接 current 之前；与 install_archive 一致）
        if let Some(def) = ctx.desc.lua_def() {
            crate::lua_plugin::hook_post_install(
                def,
                &install_dir.to_string_lossy(),
                ctx.version,
                ctx.distribution,
            );
        }

        // 4. 安装后验证（verify_bin 为空 = 跳过运行验证）
        if ctx.desc.verify_bin().is_empty() {
            tracing::info!(
                "[{}] {} 安装成功（verify_bin 为空，跳过运行验证）",
                ctx.desc.name(),
                ctx.version
            );
        } else {
            let ver = verify_install(ctx.desc, &install_dir)?;
            tracing::info!("[{}] {} 安装成功，验证输出: {ver}", ctx.desc.name(), ctx.version);
        }
        Ok(install_dir)
    })();

    // 失败清理：删除可能残留的安装目录（主文件与附加文件均已移入，一并清理）
    if result.is_err() {
        if let Err(e) = std::fs::remove_dir_all(&ctx.install_dir()) {
            tracing::warn!("清理失败安装残留目录失败: {e}");
        }
    }
    result
}

/// 把已下载文件移入安装目录（跨卷回退 copy + 删除源）
fn move_into_install(src: &Path, dest: &Path, tool: &str, version: &str) -> Result<()> {
    match std::fs::rename(src, dest) {
        Ok(()) => Ok(()),
        Err(e) if matches!(e.raw_os_error(), Some(17) | Some(18)) => {
            // 跨卷 rename 失败：回退 copy + 删除源
            tracing::warn!("[{tool}] {version} 跨卷 rename 失败（{e}），回退 copy + 删除源");
            std::fs::copy(src, dest)?;
            let _ = std::fs::remove_file(src);
            Ok(())
        }
        Err(e) => Err(EnvHiveError::with_source(
            EnvHiveErrorKind::Io,
            format!("[{tool}] {version} 安装文件失败: {}", src.display()),
            e,
        )),
    }
}

/// 安装后验证：运行可执行文件的版本命令（roadmap D：verify_install）
/// Windows 上 Defender 实时防护可能短暂锁定新解压的 exe（拒绝访问 os error 5），
/// 启动失败时指数退避重试（最多 5 次 / ~4.5s），仍失败才报错。
pub fn verify_install(desc: &dyn ToolDescriptor, install_dir: &Path) -> Result<String> {
    // verify_bin 为空 = 解压即用，不进行运行验证（防御：任何调用路径都安全）
    if desc.verify_bin().is_empty() {
        return Ok(String::new());
    }
    let mut exe = install_dir.join(desc.verify_bin());
    #[cfg(windows)]
    {
        // Windows 平台适配：
        // 1) Unix 脚本（verify_bin 以 .sh 结尾，如 tomcat 的 bin/catalina.sh）→ 映射为同名
        //    .bat（Tomcat 官方在 Windows 只分发 catalina.bat，不存在 catalina.sh）。
        // 2) 无扩展名 → 依次探测 .exe（node/python/go/java 等二进制）→ .cmd（如 maven 的
        //    bin/mvn.cmd）→ .bat（无扩展名场景下的 tomcat bin/catalina.bat）。
        let is_unix_script = exe
            .extension()
            .map(|e| e.eq_ignore_ascii_case("sh"))
            .unwrap_or(false);
        if is_unix_script {
            exe = exe.with_extension("bat");
        } else if exe.extension().is_none() {
            for ext in ["exe", "cmd", "bat"] {
                let cand = exe.with_extension(ext);
                if cand.exists() {
                    exe = cand;
                    break;
                }
            }
        }
    }
    if !exe.exists() {
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::Install,
            format!("安装验证失败：{} 不存在（安装目录 {}）", exe.display(), install_dir.display()),
        ));
    }

    const MAX_ATTEMPTS: u32 = 5;
    let mut last_err: Option<EnvHiveError> = None;
    for attempt in 0..MAX_ATTEMPTS {
        let mut cmd = std::process::Command::new(&exe);
        // verify_arg 可空（如 tomcat 的 version.bat/sh 无参，内部即输出版本）
        if !desc.verify_arg().is_empty() {
            cmd.arg(desc.verify_arg());
        }
        // cwd 设为安装目录：bat 脚本（如 tomcat 的 catalina.bat）依赖 `%cd%`
        // 推导 CATALINA_HOME，不设置则推导失败（"not defined correctly"）。对其他 工具 无影响。
        cmd.current_dir(install_dir);
        envhive_core::util::hide_console(&mut cmd); // 隐藏子进程控制台窗口（Windows）
        match cmd.output() {
            Ok(output) => {
                if output.status.success() {
                    let text = output_text(&output);
                    let first_line = text.lines().next().unwrap_or(&text).trim().to_string();
                    tracing::info!("安装验证通过：{}（{}）", exe.display(), first_line);
                    return Ok(first_line);
                }
                // 进程跑起来了但退出码非 0：不再重试（真正的验证失败）
                return Err(EnvHiveError::new(
                    EnvHiveErrorKind::Install,
                    format!("安装验证失败（{} 退出码 {:?}）：{}", exe.display(), output.status.code(), output_text(&output)),
                ));
            }
            Err(e) => {
                // 启动被拒绝（Defender 扫描锁 / 权限）：退避重试
                last_err = Some(EnvHiveError::with_source(
                    EnvHiveErrorKind::Install,
                    format!("运行 {} 失败（第 {}/{} 次尝试）", exe.display(), attempt + 1, MAX_ATTEMPTS),
                    e,
                ));
                let wait_ms = 300u64 * (attempt as u64 + 1);
                tracing::warn!("启动 {} 失败，{}ms 后重试（可能被实时防护扫描锁定）", exe.display(), wait_ms);
                std::thread::sleep(std::time::Duration::from_millis(wait_ms));
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        EnvHiveError::new(EnvHiveErrorKind::Install, format!("运行 {} 失败", exe.display()))
    }))
}

/// 卸载：删除版本目录 + 移除对应 current 链接（roadmap C：更新已安装清单）
pub fn uninstall_version(paths: &PathMeta, desc: &dyn ToolDescriptor, version: &str) -> Result<()> {
    let dir = paths.version_dir(desc.name(), version);
    if !dir.exists() {
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::NotInstalled,
            format!("{} {} 未安装", desc.name(), version),
        ));
    }
    std::fs::remove_dir_all(&dir)?;
    tracing::info!("已删除版本目录 {}", dir.display());
    Ok(())
}

/// 返回某 工具 已安装的版本目录名列表（`v-*` 前缀）
pub fn installed_versions(paths: &PathMeta, tool: &str) -> Vec<String> {
    let dir = paths.tool_cache_dir(tool);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    rd.filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.strip_prefix("v-").map(|v| v.to_string())
        })
        .collect()
}

/// 平台后缀（供 UI 展示 / 日志）
pub fn platform_suffix() -> String {
    let (os, arch) = current_platform();
    let os = match os {
        crate::tool::provider::PlatformOs::Windows => "win",
        crate::tool::provider::PlatformOs::Linux => "linux",
        crate::tool::provider::PlatformOs::Macos => "mac",
        crate::tool::provider::PlatformOs::Other => "unknown",
    };
    let arch = match arch {
        crate::tool::provider::PlatformArch::X64 => "x64",
        crate::tool::provider::PlatformArch::Arm64 => "arm64",
        crate::tool::provider::PlatformArch::Other => "unknown",
    };
    format!("{os}-{arch}")
}
