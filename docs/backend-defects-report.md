# 后端 Rust 代码缺陷审查报告

> 审查范围：`src-tauri/src/**`（Tauri 2 后端，约 50 个 .rs 文件）
> 方法：静态走读 + `cargo check`（0 编译错误）+ 对每条高危发现逐一对照源码核实
> 结论：代码可编译，但存在若干会造成**数据损坏 / 崩溃 / 功能静默失效**的真实缺陷

---

## 严重程度：高（数据损坏 / 崩溃）

### 1. Windows 注册表读取吃掉字符串最后一个字符 → 每次切换版本写坏 PATH
- 文件：`src-tauri/src/env/registry_windows.rs:57-62`（`get_user_env_var`）
- 问题：先用 `while buf.last()==Some(&0){buf.pop();}` 在**原始字节**上去尾部 0，再 `from_utf16_lossy`。但 UTF-16LE 中每个 ASCII 字符的高字节就是 `0x00`，所以会多弹掉最后一个字符的高字节，使 `buf` 长度为奇数；`buf.len()/2` 再丢掉最后一个字符的低字节。例：`C:\bin` → 读成 `C:\bi`。
- 后果：`update_user_path`（经 `sync_user_env` 在每次 `switch_global`/`apply_global_env` 调用）读出被截断的 PATH，原样写回注册表 → **用户 PATH 每次切换版本都被削掉最后一个目录项的一个字符**，长期累积可破坏 PATH。
- 另：`bytemuck_slice_u16` 用 `buf.as_ptr() as *const u16` 属未对齐读取（潜在 UB）。
- 修复：先按 `u16` 解码再去尾零：`let w: Vec<u16> = buf.chunks_exact(2).map(|c| u16::from_le_bytes([c[0],c[1]])).collect(); while w.last()==Some(&0){w.pop();}` 再 `String::from_utf16_lossy(&w)`。

### 2. 镜像源“还原”功能永久失效（备份文件名不匹配）
- 文件：`src-tauri/src/registry/mod.rs:97`（写）与 `:106`（查）
- 问题：`backup_file` 用 `path.with_extension(format!("envhive.bak.{ts}"))`。`Path::with_extension` 是**替换**扩展名：`config.toml` → `config.envhive.bak.<ts>`；而 `latest_backup` 拼的前缀是 `config.toml.envhive.bak.`，永远对不上。
- 后果：cargo / pip / maven / nuget / docker / pub / conda 的“还原”全部报“无备份可还原”，且备份文件堆积成孤儿（仅 `.npmrc`/`.gemrc`/`.condarc` 无扩展名者侥幸正确）。
- 修复：`path.with_file_name(format!("{}.envhive.bak.{ts}", path.file_name()...))`。

### 3. Maven settings.xml 注入生成非法 XML + 覆盖用户镜像
- 文件：`src-tauri/src/registry/maven.rs:94-97`（`merge_settings_xml`）
- 问题：已有 `<mirrors>` 时，用 `inner = "<mirrors>\n{mirror_block}\n  "`（无闭合标签）拼 `content[..start] + inner + content[end..]`，而 `end` 已越过 `</mirrors>` → **`</mirrors>` 闭合标签被丢弃**，且整段 `<mirrors>` 内容被单个新 mirror 整体替换。
- 后果：已有 `<mirrors>` 的用户，settings.xml 变成未闭合 XML，maven 直接解析失败、构建全挂；用户原有的其他 mirror 也一并丢失。
- 另：`content.find("</mirrors>").unwrap()` 在文件缺闭合标签时会 **panic**。
- 修复：只在段内替换 `<mirror>` 子元素，保留 `<mirrors>...</mirrors>`；用 `if let Some(..)` 代替 `unwrap()`。

### 4. 开机自启动读取会崩溃 + 状态永远显示“未开启”
- 文件：`src-tauri/src/autostart.rs:80-86`（崩溃）与 `:14` 对比 `:117`（状态错误）
- 问题①：与 #1 同源的 UTF-16 截断，使 `buf` 长度为奇数；`buf.chunks(2).map(|c| u16::from_le_bytes([c[0],c[1]]))` 中 `c[1]` **索引越界 panic**（`is_enabled()` 在 Run 值存在时必崩，exe 路径以 `.exe`/`--autostart` 的 `t`/`e` 结尾必然触发）。
- 问题②：`set_run_value` 写入 `format!("{exe} --autostart")`，而 `is_enabled` 却拿它与 `exe.to_string_lossy()` 比较 → 永不相等。
- 后果：读取开机自启动状态会崩溃；即便不崩，UI 也永远显示“未开启”。
- 修复：先解码成 `u16` 再去尾零；比较时 `v.eq_ignore_ascii_case` 前先 `v.trim_end_matches("--autostart").trim()`（或 `v.starts_with(exe)`）。

### 5. 系统代理联动在 Windows 上永远探测不到
- 文件：`src-tauri/src/sysproxy.rs:148-157`（`read_windows_proxy`）
- 问题：与 #1 同源 UTF-16 截断 → `buf` 奇数 → `if buf.len() % 2 == 0` 分支不进 → `info.raw = None` → `apply_system_proxy` 恒返回 `Ok(false)`。
- 后果：“跟随系统代理”功能在 Windows 上完全失效。
- 修复：同 #1，先 `u16` 解码再去零。

---

## 严重程度：中

### 6. 版本排序/“最新版”对带 `+build` 的版本判断错误（Adoptium/Temurin）
- 文件：`src-tauri/src/tool/version.rs:78-86`（`split_version` 的 `segs`）
- 问题：版本段仅按 `['.', '_']` 拆分，`21.0.10+7` 第三段解析为 `Seg::Str("10+7")`；与 `21.0.9+9` 做字典序比较 → 误判 `21.0.10+7 < 21.0.9+9`。
- 后果：Java（Temurin/Adoptium）的 `latest`、版本排序、前缀取最高全部错误。
- 修复：拆分符加入 `'+'`，或先按 `'+'` 剥离 build metadata。

### 7. 标签解析 `lts` 永远失败
- 文件：`src-tauri/src/tool/resolver.rs:74-80`（`matches_tag`）+ `:46-60`
- 问题：`matches_tag(Lts)` 检查版本**字符串**是否包含 `"lts"`；但真实 LTS 信息在 `AvailableVersion.labels`（provider 层），`resolve()` 从不传入 → `resolve(versions,"lts")` 恒返回 `VersionResolve` 错误。
- 修复：让 resolver 接收 `&[AvailableVersion]`，用 `labels` 判定标签。

### 8. 下载队列取消竞态（已取消的排队任务仍会安装）
- 文件：`src-tauri/src/queue.rs:167-184`（`run_worker`）
- 问题：worker 在步骤②克隆 Queued 任务（持锁），又在步骤③用**另一次加锁**将其改为 Running；两者之间的窗口内若有 `cancel()` 把该任务置 `Cancelled`，会被步骤③的 `t.status = Running` 覆盖 → 已取消的排队任务照常安装并标记 `Done`。
- 修复：在**同一次锁内**完成“选中任务 + 置 Running”。

### 9. cargo 镜像在全新机器上静默无效
- 文件：`src-tauri/src/registry/cargo.rs:127-160`（`merge_cargo_config`）
- 问题：仅在遇到 `[source.crates-io]` 时改 `replace-with`；文件不存在/无该段（全新机器最常见）时，循环结束后**没有补建逻辑** → 只追加了 `[source.rsproxy]`，缺 `replace-with` → cargo 仍走官方源。`read_current` 也读不到 `replace_with` → 状态永远 `None`。
- 修复：若 `!found_crates_io` 则追加 `[source.crates-io]\nreplace-with = "{mirror}"`。

### 10. `unlink = true` 被忽略（标记为不注入 PATH 的工具仍被注入）
- 文件：`src-tauri/src/toml_chain.rs:180-185`（`active_tools`） + `manager.rs` 的 `sync_user_env`/`resolve_envs`
- 问题：`active_tools()` 直接返回 `merged_tools()`，不过滤 `unlink`；而 `sync_user_env`/`resolve_envs` 仅检查“已安装 + 链接有效”，从不调用 `ToolValue::unlink()`。
- 后果：`.envhive.toml` 中 `{ version = "x", unlink = true }` 的工具仍被注入 PATH/环境变量，“取消全局注入”功能失效。
- 修复：`active_tools()` 或在调用处跳过 `unlink == true` 的工具。

### 11. `Envs::unset()` 是空操作（无法删除子进程的环境变量）
- 文件：`src-tauri/src/env/mod.rs:94-100`（`as_map`）
- 问题：注释声明 `vars: None` 表示“删除该变量”，但 `as_map()` 只 `insert` `Some(v)`，直接丢弃 `None` → `unset()` 注入的 `None` 不产生任何效果，变量在子进程中仍继承自父进程。
- 修复：`as_map` 需能表达“删除”（例如对 `None` 键调用 `cmd.env_remove`，但当前返回 `HashMap` 无法表达；可改为产出“待删列表”或在调用处处理）。

---

## 严重程度：低

### 12. “新版本通知”里的 latest 取值错误
- 文件：`src-tauri/src/manager.rs:264`（`check_new_versions`）
- 问题：`let latest = all.last().cloned()` 假设列表已升序；但 nodejs / github 列表为“新→旧”，`last()` 取到的是**最老**版本。
- 修复：用 `crate::tool::resolver::latest(&all)`。

### 13. `expand_tilde` 不支持 `~\`
- 文件：`src-tauri/src/util/mod.rs:9`
- 问题：只处理 `~/`，Windows 风格 `~\sdks` 落入 else 分支 → 生成名为 `~` 的真实目录。
- 修复：补 `else if let Some(rest) = path.strip_prefix("~\\")`。

### 14. `safe_component` 未拦截冒号 `:`（Windows 路径穿越）
- 文件：`src-tauri/src/util/mod.rs:19-26`
- 问题：未拒绝 `:`；`base.join("D:evil")` 在 Windows 上会丢弃 base 变成 `D:evil`。
- 修复：`&& !name.contains(':')`。

### 15. `replace_dir` 跨卷 rename 失败无回退
- 文件：`src-tauri/src/util/mod.rs:71`（`replace_dir`）
- 问题：`fs::rename` 在 src/dst 不同盘符时返回 `ERROR_NOT_SAME_DEVICE` 且无 copy 回退 → 安装目录与临时目录不同盘时安装必失败。
- 修复：失败时回退到 copy + remove。

### 16. pip 缺失键被追加到文件末尾而非目标 section
- 文件：`src-tauri/src/registry/npm.rs:221-225`（`merge_ini_section`）
- 问题：`has_section` 为真但某 key 未在段内命中时，updates 在循环外 `push` 到 `out` 尾部，若文件形如 `[global]...[install]...`，新写的 `index-url=` 会落在 `[install]` 段内 → pip 镜像静默不生效。
- 修复：记录 section 结束行号，在段内插入。

---

## 已核实为误报（未采纳）
- `registry/proxy.rs` “关闭代理仍写入代理”：核实 `inject_tool_proxy` 通过 `clear = !proxy.enable` 正确移除代理键，功能正常。仅 `strict-ssl=false` 在代理启用时无条件写入（属自签 corporate 代理的**设计取舍**，非缺陷）。
- `pathmeta.rs` “`init` 与 `from_root` 缓存目录不一致”：核实两者均解析为 `root/installs/versions`，一致。

## 编译状态
`cargo check`：0 错误、0 新增警告（依赖已缓存，增量校验通过）。以上均为逻辑/行为缺陷，非编译错误。

---

## 修复状态（2026-08-07）：全部 16 项已修复，`cargo test --lib` 44 passed / 0 failed

| # | 缺陷 | 修复位置 | 修复要点 |
|---|------|----------|----------|
| 1 | 注册表读值吃最后字符 | `env/registry_windows.rs` `get_user_env_var` | 仅在末尾成对去除 `0x00`（完整 UTF-16 空字符），不再逐字节弹，并补偶数长度保护 |
| 2 | 镜像还原永久失效 | `registry/mod.rs` `backup_file` | 改为在完整路径后**追加** `.envhive.bak.<ts>` 后缀（保留原扩展名），与 `latest_backup` 前缀匹配 |
| 3 | Maven 注入非法 XML | `registry/maven.rs` `merge_settings_xml` | 整体替换 `<mirrors>…</mirrors>` 块，保留闭合标签；`has_mirrors` 需 `<mirrors>` 与 `</mirrors>` 成对 |
| 4 | 自启动崩溃+状态错 | `autostart.rs` | 同 #1 的 UTF-16 去尾修复；`is_enabled` 同时接受 `exe` 与 `exe --autostart` 两种形式 |
| 5 | 系统代理探测失效 | `sysproxy.rs` `read_windows_proxy` | 同 #1 的 UTF-16 去尾修复 |
| 6 | 版本 `+build` 排序错 | `tool/version.rs` `split_version` | 先剥离 `+build` 元数据再分段比较（遵循 semver） |
| 7 | `lts` 标签解析失败 | `tool/resolver.rs` + `manager.rs` | 新增 `resolve_versions_meta`，基于 `AvailableVersion.lts`/`labels` 解析标签；`resolve_version` 可用列表分支改用它 |
| 8 | 队列取消竞态 | `queue.rs` `run_worker` | 取任务与置 `Running` 合并到**同一把锁**内，消除取消窗口 |
| 9 | cargo 镜像静默无效 | `registry/cargo.rs` `merge_cargo_config` | 结果缺 `[source.crates-io]` 时补建 `replace-with` |
| 10 | `unlink=true` 被忽略 | `toml_chain.rs` `active_tools` | 过滤掉 `unlink()` 为真的工具，不再注入 PATH |
| 11 | `Envs::unset` 空操作 | `env/mod.rs` + `manager.rs` | 新增 `apply_to_command`：`Some`→`env` 覆盖，`None`→`env_remove` 真正删除；两个 spawn 路径改用它 |
| 12 | 新版本通知 latest 错 | `manager.rs` `check_new_versions` | 改用 `resolver::latest(&all)` 取真正最高版本 |
| 13 | `expand_tilde` 不支持 `~\` | `util/mod.rs` | 补 `~\\` 前缀分支 |
| 14 | `safe_component` 未拦 `:` | `util/mod.rs` | 增加 `!name.contains(':')` 防 Windows 路径穿越 |
| 15 | `replace_dir` 跨卷无回退 | `util/mod.rs` `replace_dir` | `rename` 失败（ERROR_NOT_SAME_DEVICE/EXDEV）时回退 copy + 删源 |
| 16 | pip 缺失键写到错误段 | `registry/npm.rs` `merge_ini_section` | 离开目标 section 时把未命中键补写到段末尾，而非文件尾 |

> 新增回归测试：`tool::version::tests::test_build_metadata_ignored`、`tool::resolver::tests::test_resolve_lts_meta`、`registry::maven::tests::test_maven_replace_valid_xml`，均通过。
