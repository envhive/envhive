# envhive Tauri 后端拆 Crate 评估报告

> 日期：2026-08-14
> 范围：`app/src-tauri`（约 1.26 万行 Rust，47 个单元测试）

## 一、结论

**可以拆分，且值得拆。** 当前代码已在单 crate 内按功能模块做了良好的子模块划分（`tool` / `plugin` / `lua_plugin` / `registry` / `manager` / `commands`…），模块边界清晰，这为拆分提供了很好的基础。

但拆分前**必须先解开 3 处循环依赖**（见下文），否则 crate 之间会出现循环引用，Rust 编译器不允许 crate 之间形成环。拆完后单 crate 编译变成 workspace 多 crate 增量编译，编译速度、模块强制隔离、单元测试粒度都会改善。

## 二、现状分析

### 2.1 规模与结构

- 单 crate `envhive`（lib 名 `envhive_lib`），总代码约 **12,584 行**，45 个源文件，47 个 `#[test]`
- 依赖：Tauri 2 + tokio + reqwest + mlua（Lua VM）+ serde 等，约 200+ 传递依赖

### 2.2 现有模块分层（已经很好）

| 层级 | 模块 | 说明 |
|---|---|---|
| UI/壳 | `commands/`（7 文件）、`lib.rs`、`main.rs` | 71 处直接调用 manager |
| 编排 | `manager/`（6 文件）、`extras.rs` | `EnvHiveManager` 聚合所有域 |
| 业务域 | `tool/`（9 文件，约 2,700 行）、`plugin.rs`（466 行）、`lua_plugin/`（5 文件，约 1,760 行）、`registry/`（10 文件，约 1,800 行）、`mirror.rs`、`queue.rs`、`usage.rs` | 核心业务逻辑 |
| 基础设施 | `error.rs`、`config.rs`、`pathmeta.rs`、`util/`、`events.rs`、`logging.rs`、`toml_chain.rs`、`env/`、`autostart.rs`、`projects.rs` | 无业务依赖或低依赖 |

### 2.3 依赖热度（被引用次数）

```
error:49  tool:57  lua_plugin:26  config:21  plugin:19  pathmeta:17
env:17  util:18  projects:11  extras:10  registry:8  mirror:8
queue:4  usage:4  autostart:2  events:6  toml_chain:5  logging:0
```

`error`、`tool`、`lua_plugin`、`config` 是被引用最多的共享基础，天然适合下沉为底层 crate。

## 三、拆分的主要障碍：3 处循环依赖

Rust 的 crate 之间**不允许循环依赖**（编译报 E0590 环检测），所以拆之前必须先解开以下环：

### 循环 ①  tool ↔ lua_plugin

- `tool/install.rs`、`tool/provider.rs` 调用 `crate::lua_plugin::hook_post_install / hook_available / resolve_lua_package / set_plugin_proxy`
- `lua_plugin/mod.rs` 实现 `crate::tool::Tool` trait、调用 `tool::install::install_binary`

**解环方案**：把 `Tool` / `ToolDescriptor` trait 及其依赖的共享类型（`DistributionInfo`、`MirrorCandidate`、`EnvVarKind`、`PlatformMap`、`PluginDef`、`ProviderKind`）下沉到 `envhive-core`；`lua_plugin` 通过实现 trait 反向供 `tool` 调用，不再直接互相 `use`。

### 循环 ②  tool ↔ plugin

- `tool/provider.rs:608` 构造 `crate::plugin::PluginDef`
- `plugin.rs` 实现 `crate::tool::Tool` trait、引用 `tool::icon`

**解环方案**：同上，`PluginDef` 及 `Tool` trait 下沉到 `envhive-core`，两个域都只依赖 core。

### 循环 ③  manager ↔ usage（+ queue → manager）

- `usage.rs` 使用 `EnvHiveManager` 统计
- `manager/tool.rs` 调用 `crate::usage::record_use`
- `queue.rs` 的 worker 直接持有 `EnvHiveManager`

**解环方案**：`usage` 的统计逻辑改为只依赖 `pathmeta`（路径）而不依赖 manager；`queue` 的 worker 改为接收 `Arc<dyn TaskExecutor>` trait 对象（由 manager 实现），或把 queue 并入 manager crate。

## 四、建议的目标架构（5 个 crate）

```
┌─────────────────────────────────────────────┐
│ envhive-app   Tauri 壳（main/lib/commands）  │
│   只做命令注册 + UI 粘合，不含业务逻辑        │
└───────────────┬─────────────────────────────┘
┌───────────────▼─────────────────────────────┐
│ envhive-manager  编排层                      │
│ EnvHiveManager / queue / usage / env        │
│ 组合各域：安装、切换、环境注入、统计          │
└───────┬──────────────────────┬──────────────┘
┌───────▼──────────┐   ┌───────▼──────────────┐
│ envhive-toolkit  │   │ envhive-plugin       │
│ 工具域            │   │ 插件域               │
│ tool/ registry/  │   │ lua_plugin/ plugin/  │
│ mirror/          │   │ 实现 Tool trait+LuaVM│
└───────┬──────────┘   └───────┬──────────────┘
┌───────▼──────────────────────▼──────────────┐
│ envhive-core  共享地基（零业务依赖）          │
│ error/config/pathmeta/events/logging        │
│ Tool/ToolDescriptor trait + 共享类型         │
└─────────────────────────────────────────────┘
```

依赖方向严格单向：`app → manager → {toolkit, plugin} → core`。

### 各 crate 职责与迁移清单

| Crate | 迁入模块 | 新增对外 API | 风险 |
|---|---|---|---|
| `envhive-core` | error, config, pathmeta, util, events, logging, toml_chain + tool 的 trait/共享类型 | `pub` 化全部类型，`bail!` 宏重新导出 | 低（纯搬移 + 可见性） |
| `envhive-toolkit` | tool/（实现部分）, registry/, mirror/ | 对外暴露 `install/download/version` 函数 | 中（需解环 ①②） |
| `envhive-plugin` | lua_plugin/, plugin.rs | 注册 Tool trait 实现 | 中（需解环 ①②） |
| `envhive-manager` | manager/, queue.rs, usage.rs, env/, autostart.rs, projects.rs | `EnvHiveManager`、队列 worker | 中（需解环 ③） |
| `envhive-app` | commands/, lib.rs, main.rs, extras.rs | Tauri 命令注册 | 低 |

> 注：`extras.rs`（插件同步胶水层）依赖 9 个模块，建议拆散：同步逻辑进 `manager`，纯网络逻辑进 `toolkit`。

## 五、迁移步骤（建议顺序）

1. **先搭 workspace 骨架**：根目录建 `Cargo.toml`（`[workspace] members`），`src-tauri` 改为 workspace 成员。
2. **第一步：拆 `envhive-core`**（风险最低，收益最大）
   - 纯搬移 `error/config/pathmeta/util/events/logging/toml_chain`
   - 把 `Tool`/`ToolDescriptor` trait 及共享类型从 `tool/mod.rs` 抽出放入 core
   - 全项目替换 `crate::xxx` → `envhive_core::xxx`，编译通过即完成
3. **第二步：拆 `envhive-plugin` 与 `envhive-toolkit`**（并行解环 ①②）
   - plugin/lua_plugin 实现 core 中的 trait
   - tool 只依赖 core，hook 通过 trait 方法回调
4. **第三步：拆 `envhive-manager`**（解环 ③）
   - `usage` 改为只依赖 `pathmeta`；`queue` worker 用 trait 对象
5. **第四步：拆 `envhive-app`**（收尾）
   - commands 只调 manager，天然无环，最后切最安全

每一步都以「编译通过 + 47 个单元测试通过」为完成标志，可逐步合入。

## 六、收益与成本

### 收益
- **编译加速**：现在改一行命令触发整个 crate 重编译；拆分后增量编译，core/toolkit 稳定后基本不重编
- **强制边界**：跨域访问必须走 `pub` API，杜绝 `dead_code` 式隐性耦合（当前 lib.rs 顶部有 `#![allow(dead_code)]`，说明存在未消费的 P0 骨架代码，拆分时会被迫清理）
- **测试粒度**：core/toolkit 可脱离 Tauri 直接 `cargo test`
- **可复用**：core/toolkit 未来可被 CLI 版 envhive 复用

### 成本与风险
- 迁移工作量大：约 45 个文件都要改 `use` 路径和可见性
- 循环依赖解环涉及 trait 下沉设计，需要谨慎（核心风险点）
- `tauri.conf.json`、`build.rs`、`capabilities/` 中如有路径假设需同步调整
- 测试 fixture 路径（`CARGO_MANIFEST_DIR/../../plugins`）在 workspace 布局下会变化，需用 `workspace_root` 宏或环境变量修正
- git 历史会因大范围搬移而模糊（建议按目录移动而非新建，git 能识别 rename）

## 七、备选方案（如果不想大动）

如果近期以功能迭代为主，可以只做 **两步轻量拆分**：
1. 只拆 `envhive-core`（零环，纯收益）
2. 保持 tool/plugin/lua_plugin 在同一个 crate（它们之间耦合深，拆分收益有限）

剩下 3 个 crate 等架构稳定、版本 1.0 前后再拆。

## 八、量化参考

| 指标 | 当前 | 拆分后 |
|---|---|---|
| crate 数 | 1 | 4（envhive / core / toolkit / manager） |
| 最大 crate 代码量 | ~12,584 行 | ~6,400 行（toolkit） |
| 循环依赖 | 3 处（模块级） | 0（crate 级强制） |
| 命令层 → manager 调用 | 71 处 | 71 处（不变，但必须走 pub API） |
| 单元测试 | 47 | 52（core 3 + toolkit 42 + manager 7，全部通过） |

## 九、实施记录（2026-08-14 完成）

### 实际拆分结果：4 个 crate（比原计划 5 个少 1 个，原因见下）

```
app/src-tauri/
├── Cargo.toml                 # workspace 根（members: crates/*）
├── src/                       # envhive（Tauri 壳）1,319 行
│   ├── main.rs / lib.rs       # 仅入口 + 命令注册 + 重导出
│   └── commands/              # 7 个命令文件，71 处调用 manager
└── crates/
    ├── envhive-core/          # 1,815 行 — error/config/pathmeta/util/toml_chain/logging/env/events(类型)
    ├── envhive-toolkit/       # 6,421 行 — tool/plugin/lua_plugin/registry/mirror（工具与插件域）
    └── envhive-manager/       # 3,097 行 — manager/queue/usage/autostart/projects/extras/events(发射)
```

依赖方向严格单向：`envhive → envhive-manager → envhive-toolkit → envhive-core`。

### 与原计划的差异与原因

1. **tool/plugin/lua_plugin 未拆成两个 crate，而是并入 envhive-toolkit**
   - 三者是**设计使然的内聚域**：`Tool`/`ToolDescriptor` trait 直接引用 `PluginDef`/`LuaPluginDef` 类型（`platform()` / `lua_def()`），且安装流程互相调用 hook（`tool::install` 调 `lua_plugin::hook_post_install`，`lua_plugin` 又实现 `Tool` trait 调 `tool::install`）。
   - 拆开需把 trait 纯接口化 + hook 依赖注入，重构成本远大于收益。合并为「工具与插件域」是最优解。
2. **envhive-manager 保留了 Tauri 依赖**（原计划"零 Tauri"）
   - manager 的安装/切换方法直接持 `&AppHandle` 做进度事件推送（5 处），解耦需引入事件回调 trait，改动面大。
   - 收益权衡后，让 manager 作为**依赖 tauri 的编排层 crate** 是更干净的边界：CLI 复用 core/toolkit 即可，无需 manager。

### 迁移中的关键处理（供后续参考）

- **`pub use` 重导出保路径**：主 crate `lib.rs` 用 `pub use envhive_core::{...}` 重导出全部内部模块，使 `crate::xxx` 路径零改动，迁移成本集中在搬文件本身。
- **孤儿规则（E0117）**：`impl From<mlua::Error> for EnvHiveError` 在拆分后违反孤儿规则（EnvHiveError 变外部类型），改为 `lua_err()` 本地转换函数 + `map_err(lua_err)`。
- **`pub(crate)` → `pub`**：`util::copy_dir`、`lua_plugin::build_plugin_def` 等被跨 crate 使用，需放开可见性。
- **`#[macro_export]` 宏**：`bail!` 通过 `pub use envhive_core::bail` 重导出保持 `use crate::bail` 可用。
- **测试 fixture 路径**：`CARGO_MANIFEST_DIR` 随 crate 迁移变化，toolkit/manager 内 `../../plugins` 改为 `../../../../plugins`（上溯 4 级到仓库根）。
- **下载进度解耦**：`Downloader.app: Option<&AppHandle>` 改为 `progress: Option<&dyn Fn(&DownloadProgress)>` 回调，使 toolkit 零 Tauri 依赖；manager 侧注入闭包 `|p| events::emit_progress(app, p)`。
- **事件类型下沉**：`DownloadProgress`/`DownloadStage`/`InstallStatus`/`VersionChanged` 纯类型放入 `envhive-core::events`，`events.rs`（Tauri 发射）留在 manager。

### 环境备注（Windows 编译）

- 本机运行 360 安全卫士，会拦截 `target/` 目录下的文件锁/写入（`os error 5`），导致 `cargo check/test` 偶发失败。
- 绕过方法：`cargo check --target-dir C:\Users\cuijian\AppData\Local\Temp\envhive-target`（Temp 目录不受拦截）；原始 `target/` 在删除 `.cargo-*.lock` 后间歇可用。
- xz2/lzma-sys 在全新 Temp target 下偶发链接失败（LNK2019），用原始 `target/` 跑测试可绕过。

---

*评估与实施基于 2026-08-14 的代码快照；若后续 tool/plugin 间新增直接调用，需先评估是否引入新的循环。*
