# envhive CLI（envhive-cli）设计草案

> 状态：**已实施（2026-08-14）—— 首期 `init` + `load` 已完成，Shell Hook 为演进项**
> 目标：新增一个与桌面应用共存的 CLI 可执行文件 `envhive-cli`，支持在指定项目目录生成工具配置文件（`.envhive.toml`），并在当前命令窗口执行 `envhive-cli load` 加载配置中的工具环境。
> 对应 roadmap 模块 F「Shell Hook 与 Session 落地」（[roadmap.md](roadmap.md)）的 CLI 辅助路线；roadmap 原规划命令为 `envhive activate` + `envhive env -s <shell>`，本文收敛为 `init` + `load`（`load` ≈ `env` 的"输出注入脚本"动作）。
>
> **实施落地（2026-08-14）**：crate 拆分完成后，`envhive-cli` 作为独立 crate 实现于 `app/src-tauri/crates/envhive-cli`，复用 envhive-core（配置链/路径/错误）+ envhive-toolkit（工具查找 `ToolLookup`、环境解析 `resolve_envs`、Shell 渲染 `shell::render`），零 Tauri 依赖。
> **迁移（2026-08-19）**：CLI 已完全脱离 tauri 项目，移至仓库根 `cli/`（共享 crate 移至根 `crates/`，根 workspace 管理）；新增 `tui` 交互命令（ratatui：工具/插件/队列三 Tab + 下载进度条）与 `install` / `switch` / `unuse` / `list` 非交互命令；桌面端不再捆绑 CLI（externalBin 移除）。
> 新增 toolkit 模块：`env_resolver.rs`（配置链 → 合并 Envs，桌面 manager::resolve_envs 已改为复用它）、`shell.rs`（bash/zsh/fish/powershell/cmd 五种 shell 的 export/unset 脚本生成 + 转义，含单元测试）。
> 实测：四种 shell 输出均正确（`export PATH="<new>${PATH:+:$PATH}"` / `$env:PATH = "...;" + $env:PATH` / `set "PATH=...;%PATH%"` / `set -gx PATH '...' $PATH`）。

---

## 1. 背景与现状（代码核实）

**核心结论：后端机制已就绪，缺口仅在「CLI 可执行文件」与「写项目配置的入口」。**

| 能力 | 现状 | 位置 |
|---|---|---|
| `.envhive.toml` 解析 / 序列化 / 原子写回 | ✅ 已实现 | `src-tauri/src/toml_chain.rs`（`ScopeConfig::from_file` / `save_to`） |
| 项目配置自动定位（从 cwd 向上找 `.envhive.toml`） | ✅ 已实现 | `toml_chain.rs::find_project_toml` |
| Global / Project / Session 三作用域合并链 | ✅ 已实现 | `toml_chain.rs::ConfigChain`（后者优先） |
| 合并 env 计算（PATH 前置 / JAVA_HOME 等） | ✅ 已实现 | `manager.rs::resolve_envs(project_dir)` |
| bash 风格 export 脚本生成 | ✅ 已实现 | `commands/mod.rs::preview_env`（可直接提取复用） |
| 无 Tauri 依赖地重建 PathMeta（CLI 复用预留） | ✅ 已实现 | `pathmeta.rs::from_root`（注释即写明"供测试 / CLI 复用"） |
| CLI 可执行文件 | ❌ 不存在 | `Cargo.toml` 无 `[[bin]]` / clap；`main.rs` 为 `windows_subsystem = "windows"` 纯 GUI 入口 |
| 在指定目录写入项目级 `.envhive.toml` | ❌ 不存在 | UI 项目预设存 `~/.envhive/state/projects.json`（会话注入用），非项目目录内配置文件 |

---

## 2. 目标与非目标

### 目标

- 新增独立可执行文件 `envhive-cli`，**桌面应用完全不受影响**（双二进制共存，共享 `envhive_lib`）。
- `envhive-cli init [--dir <path>]`：在指定（默认当前）项目目录生成 `.envhive.toml` 配置文件。
- `envhive-cli load`：从当前目录向上定位配置 → 计算合并 env → **输出对应 shell 语法的脚本**，由当前终端执行后工具环境生效。
- 支持 bash / zsh / PowerShell / cmd 四种终端。

### 非目标（后续阶段）

- Shell Hook 自动切换（`cd` 进项目自动生效，即 roadmap 的 `activate`）——列为演进项。
- Session 临时目录（`~/.envhive/tmp/<date>-<pid>/`）的 CLI 侧落地。
- `.sdkmanrc` / `.tool-versions` 等第三方配置兼容。
- CLI 安装 / 卸载工具（首期只读配置 + 写配置，安装仍在桌面端）。

---

## 3. 架构：双二进制共享核心

```
src-tauri/
├── src/main.rs        # 桌面 GUI 入口（现有，不动）
│                      #   #![cfg_attr(windows, windows_subsystem = "windows")]
├── src/bin/cli.rs     # 新增 CLI 入口（不含 windows_subsystem 属性 → 自带控制台）
└── src/               # envhive_lib 各模块（manager / toml_chain / pathmeta ...）
```

- `Cargo.toml` 增加：

```toml
[[bin]]
name = "envhive-cli"
path = "src/bin/cli.rs"
```

- **数据共享**：CLI 与桌面读写同一份 `~/.envhive/`（`.envhive.toml`、`installs/`、`tools/`、`plugins/`），数据不分裂。
- **构建**：一次 `cargo build` / `tauri build` 同时产出桌面 `envhive.exe` 与 CLI `envhive-cli.exe`，互不干扰。
- **Windows 子系统**：桌面 exe 声明 `windows_subsystem = "windows"`（不弹黑窗）；CLI bin **不声明**，保证控制台输出正常。

---

## 4. CLI 命令设计

### 4.1 `envhive-cli init` —— 生成项目配置文件

```
envhive-cli init [--dir <path>] [--force] [--tool <name>=<version>]...
```

- 默认在**当前目录**生成 `.envhive.toml`；`--dir` 指定目标目录（自动创建）。
- 生成模板（示例）：

```toml
# envhive 项目配置
# 使用 `envhive-cli load` 将以下工具注入当前终端

[tools]
# nodejs = "22.11.0"
# java = { version = "21", vendor = "openjdk" }
# golang = { version = "1.24.1", unlink = true }
```

- `--tool nodejs=22.11.0` 可多次传入，直接写入 `[tools]`（复用 `toml_chain::ToolValue` 序列化）。
- `--force`：目标文件已存在时覆盖（默认拒绝，提示用户）。
- 复用 `ScopeConfig::save_to`（原子写回：临时文件 + rename）。

### 4.2 `envhive-cli load` —— 加载配置到当前终端

```
envhive-cli load [--shell <bash|zsh|powershell|cmd|fish>] [--dir <path>] [--json]
```

- 流程：
  1. 从当前目录（或 `--dir`）向上定位 `.envhive.toml`（`find_project_toml`）；
  2. 加载 Global + Project 配置链（`ConfigChain::load`）；
  3. 复用 `resolve_envs` 计算合并 env（含 PATH 前置、JAVA_HOME 等，跳过未安装/坏链接的工具）；
  4. 按 shell 类型输出脚本到 stdout；退出码非 0 表示加载失败（不输出半截脚本）。
- **shell 自动检测**：`--shell` 缺省时，通过父进程名（Windows：`Get-Process -Id $PID` 的父进程 / 环境变量 `$SHELL`）判断。
- `--json`：输出结构化结果（环境变量表 + 配置来源），供脚本 /工具消费。

### 4.3 与 roadmap 命令的对应关系

| roadmap（模块 F） | 本文 | 说明 |
|---|---|---|
| `envhive env -s <shell>` | `envhive-cli load` | 同一动作：计算 env 差异 → 输出 export/unset 语句 |
| `envhive activate` | 未纳入首期 | Shell Hook（cd 自动切换），演进项 |
| （无） | `envhive-cli init` | 新增：项目配置生成入口 |

---

## 5. 关键约束与各 shell 输出（核心设计）

**子进程永远无法修改父 shell 的环境变量**（操作系统限制），因此 `envhive-cli load` 必须"输出脚本 → 父 shell 自己执行"。CLI 只负责生成正确语法并做转义。

| Shell | 用户执行方式 | CLI 输出示例 |
|---|---|---|
| bash / zsh | `eval "$(envhive-cli load)"` | `export PATH="/path/.envhive/tools/nodejs/current/bin:$PATH"`<br>`export JAVA_HOME="..."`<br>`unset NODE_OPTIONS` |
| PowerShell | `envhive-cli load | Out-String | Invoke-Expression`<br>或写临时 ps1 后点源 `.` | `$env:PATH = "...;" + $env:PATH`<br>`$env:JAVA_HOME = "..."`<br>`Remove-Item Env:NODE_OPTIONS` |
| cmd | `envhive-cli load > %TEMP%\p.cmd && call %TEMP%\p.cmd` | `set "PATH=...;%PATH%"`<br>`set "JAVA_HOME=..."`<br>`set "NODE_OPTIONS="` |

- **cmd 特殊性**：cmd 没有 eval；`call` 一个批处理与当前 cmd **同一进程**运行，环境变量修改可保留——这是可利用点。CLI 可额外提供 `--emit-file <path>` 直接写临时 .cmd 并提示 `call` 命令。
- **转义**：值含空格 / 引号 / 特殊字符时按 shell 分别转义（Bash 用单引号包裹并转义内部单引号；PowerShell 用单引号 + `''`；cmd 用 `"..."` 包裹），防注入。
- **PATH 语义**：envhive 的条目**前置**，保留终端现有 `$PATH` 后缀。
- **unset 语义**：配置删除的工具/ 需清除的变量（`Envs` 中值为 None 的项）输出 unset / Remove-Item / 置空。

---

## 6. 实现清单（待确认后实施）

1. **`Cargo.toml`**：增加 `[[bin]] name = "envhive-cli"`；新增依赖 `clap`（derive 模式）。
2. **`src-tauri/src/bin/cli.rs`**：clap 入口，注册 `init` / `load` 子命令。
3. **复用现有 lib（不改动）**：
   - `pathmeta::from_root` / `PathMeta::init` —— 定位 `~/.envhive` 布局；
   - `toml_chain::find_project_toml` / `ConfigChain::load` / `ScopeConfig::save_to` —— 配置读写；
   - `manager`（构造方式同 `lib.rs::run`：`AppConfig::load` + `PathMeta::init` + reqwest client）→ `resolve_envs`；
   - `commands::preview_env` 的 export 脚本生成逻辑 —— 提取为 lib 内公共函数，按 shell 参数化。
4. **shell 检测 + 脚本生成模块**（新增，如 `src-tauri/src/cli_shell.rs`）：各 shell 语法渲染 + 转义 + 单元测试。
5. **`--json` 输出**：`serde` 序列化 env 表 + 来源标注。
6. **验证**：
   - `cargo build` 同时产出 `envhive.exe` 与 `envhive-cli.exe`；
   - `cargo test`（shell 转义、TOML 生成、find_project_toml 向上定位）；
   - 三种 shell 实测：bash / PowerShell / cmd 各跑一遍 init → load → `node -v` / `java -version` 验证。
7. **文档**：README「快速开始」补充 CLI 用法；roadmap 勾选对应项。

---

## 7. 演进项（本期不做）

- **`envhive-cli activate`**：Shell Hook。安装 hook 到当前 shell（bash `PROMPT_COMMAND` / zsh `precmd` / fish `fish_prompt` / PowerShell 重定义 `prompt`），`cd` 进项目自动 `load`；配套 `__PRISMENV_PID` / `__PRISMENV_SHELL` 环境变量 + `trap 'envhive env --cleanup' EXIT` 退出清理。
- **快速路径缓存**：记录 `.envhive.toml` mtime + PATH 指纹，未变化跳过重算（避免拖慢提示符）。
- **Session 层 CLI 落地**：`~/.envhive/tmp/<date>-<pid>/config.toml` 的读写与 `clean_tmp()` 清理。
- **配置差异（diff）输出**：仅输出与当前 shell 环境不同的变量，减少噪音（roadmap 语义：`env` 计算 env 差异）。

---

## 8. 待决策问题

1. CLI 二进制命名：`envhive-cli`（本文采用）还是复用 `envhive` 并让桌面 exe 改名？
2. `load` 首期输出**全量** env（简单、幂等）还是**差异** env（省噪音，复杂度高）？建议首期全量。
3. cmd 场景是否首期就支持 `--emit-file` 一键 call 模式，还是只输出 stdout 让用户手动重定向？
4. 是否允许 CLI 首期就提供 `--json`（为未来脚本化 / CI 预留）？
