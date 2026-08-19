# 可行性分析：插件目录工具图标（png / jpg / lua base64）

> 日期：2026-08-10 · 状态：可行，方案见下
> 需求：插件目录添加工具图标，支持 png、jpg、Lua 中 base64；首页、工具管理、插件管理、统计等所有显示工具的界面添加图标显示。

---

## 1. 需求拆解

| # | 子需求 | 说明 |
|---|---|---|
| 1 | 图标来源 | 插件目录内的 `icon.png` / `icon.jpg` 文件；Lua 插件脚本内以 base64 声明（`TOOL.icon_base64`） |
| 2 | 后端透传 | Lua/TOML 插件定义解析图标 → 序列化到 `ToolInfo` / `PluginInfo` / `ToolUsage` / `ToolEnvInfo` 等前端模型 |
| 3 | 前端渲染 | 首页、工具管理、插件管理、统计四处主界面 + 安装弹窗/预设选择器等次要界面统一显示图标 |
| 4 | 兼容性 | 无图标的旧插件回退现有彩色圆点标识，不影响现有功能 |

---

## 2. 现状分析（结论：改动面集中，无架构阻碍）

### 2.1 后端数据模型 — 全部缺少 icon 字段

| 结构体 | 位置 | 说明 |
|---|---|---|
| `LuaPluginDef` | `src-tauri/src/lua_plugin/mod.rs` |工具表字段解析在 `def.rs::build_plugin_def`，加字段只需一处 |
| `PluginDef`（TOML） | `src-tauri/src/plugin.rs` | serde 反序列化，新增可选字段自动支持 |
| `ToolDescriptor` trait | `src-tauri/src/tool/mod.rs` | 带默认实现的 trait 方法可安全扩展 |
| `ToolInfo`（工具卡片） | `src-tauri/src/tool/mod.rs` | `ToolInfo::from_desc` 统一组装 |
| `PluginInfo`（插件列表） | `src-tauri/src/plugin.rs` | Lua 侧 `lua_plugin/info.rs::to_plugin_info` 同构 |
| `ToolUsage`（统计） | `src-tauri/src/usage.rs` | 统计聚合处从 `desc()` 取 |
| `ToolEnvInfo`（首页） | `src-tauri/src/commands/home.rs` | 首页概览 |

### 2.2 前端展示位置 — 4 处主界面 + 3 处次要

| 界面 | 文件 | 当前身份标识 |
|---|---|---|
| 首页 | `src/pages/HomePage.vue` | `DOT_COLORS[i % 4]` 彩色圆点 |
|工具管理 | `src/components/ToolCard.vue`（+`ToolsPage.vue`） | 同上 |
| 插件管理 | `src/pages/PluginsPage.vue` | 无图标，纯文本（市场卡片 + 已装插件行） |
| 统计 | `src/pages/StatsPage.vue` | 柱状条名称 / 树形表名称列，无图标 |
| 次要 | `InstallToolModal.vue` / `VersionComboPicker.vue` / `ProjectEnvRow.vue` | 无图标 |

### 2.3 Lua 运行时 — 暂无 base64 能力

- 内置模块：`http.get` / `json` / `archiver.extract` / `file` / `versions.parse`（`lua_plugin/modules.rs`）；
- **无 base64 模块**（全库 grep 确认）；
- v2 可信模型已放开全部标准库，插件内自行拼 base64 理论可行，但无规范模块。

### 2.4 结论

- 需求在现有插件协议（v2：`distributions` / `mirrors` 已示范"工具表加字段 → 透传前端"的完整路径）上**完全可行**；
- 改动全部是**增量式、向后兼容**（新字段可选，缺省回退圆点），无架构风险。

---

## 3. 方案设计

### 3.1 图标来源与优先级（推荐）

```
优先级 1  插件目录文件：~/.envhive/plugins/<name>/icon.png | icon.jpg
优先级 2  Lua 插件声明：TOOL.icon_base64 = "<base64 字符串>"
优先级 3  TOML 插件声明：icon = "<base64 字符串>"（可选，TOML 无脚本可直接放）
缺省      无图标 → 前端回退现有 DOT_COLORS 彩色圆点
```

- 文件方式：对作者最友好（放个 logo 即可，无需改脚本），且 Lua/TOML 插件通用；
- base64 方式：单文件分发友好（一个 plugin.lua 携带图标，适合插件市场 manifest 打包）；
- 后端统一转为 `data:image/png;base64,...` data URI 后传给前端，前端 `<img :src>` 直接渲染，无需 Tauri asset 协议 / convertFileSrc。

### 3.2 后端改动清单（Rust）

| 改动 | 位置 | 内容 |
|---|---|---|
| 1 | `lua_plugin/def.rs` | `build_plugin_def` 解析 `TOOL.icon_base64`（可选）→ 存入 `LuaPluginDef.icon_base64: Option<String>` |
| 2 | `plugin.rs` | `PluginDef` 增加 `#[serde(default)] icon: Option<String>` |
| 3 | `tool/mod.rs` | `ToolDescriptor` 增加默认方法 `fn icon(&self) -> Option<String> { None }`；`ToolInfo` 增加 `icon: Option<String>` 并在 `from_desc` 组装 |
| 4 | 新增工具函数 | `icon_util.rs`（或并入 `tool/mod.rs`）：按 `<插件目录>/icon.png` → `icon.jpg` → 声明 base64 顺序解析，读文件 → base64 编码 → 拼 data URI；建议按文件 mtime 做内存缓存 |
| 5 | `lua_plugin/info.rs` + `plugin.rs` | `to_plugin_info` / `plugin_info` 透传 icon 到 `PluginInfo` |
| 6 | `usage.rs` | `ToolUsage` 增加 `icon: Option<String>`（从 `manager.lookup_tool(name)?.desc().icon()` 取） |
| 7 | `commands/home.rs` | `ToolEnvInfo` 增加 `icon` 字段透传 |
| 8 | `lua_plugin/modules.rs` | 新增内置模块 `base64.encode/decode`（满足"lua 中 base64"的完整语义：插件可编码/解码图标、下载 logo 转 base64 等） |
| 9 | `Cargo.toml` | 新增依赖 `base64`（小型、成熟；或手写 ~20 行避免依赖） |

### 3.3 前端改动清单（Vue）

| 改动 | 位置 | 内容 |
|---|---|---|
| 1 | `src/types.ts` | `ToolInfo` / `PluginInfo` / `ToolUsage` / `ToolEnvInfo` / `RemotePluginInfo` 增加 `icon?: string \| null` |
| 2 | 新组件 `src/components/ToolIcon.vue` | 统一渲染：有 icon → `<img :src>`（含加载失败回退）；无 icon → 彩色圆点（沿用 DOT_COLORS 逻辑） |
| 3 | `ToolCard.vue` | 头部 `dot` 圆点 → `<ToolIcon>` |
| 4 | `HomePage.vue` | mini-card 圆点 → `<ToolIcon>`（含 `ProjectEnvRow` / `VersionComboPicker` 可选接入） |
| 5 | `PluginsPage.vue` | 已装插件行 + 市场卡片身份区 → `<ToolIcon>`（市场 manifest 可携带 icon） |
| 6 | `StatsPage.vue` | 柱状条名称前、树形表工具列 → `<ToolIcon>` |
| 7 | `InstallToolModal.vue` | 弹窗标题区可选显示图标 |
| 8 | 内置插件素材 | 为 6 个默认插件（nodejs/java/go/rust/python/maven）准备官方 logo 存入 `src-tauri/plugins/<name>/icon.png`，随二进制分发（与 plugin.lua 同机制 include / 首启写入） |

### 3.4 数据流（一次改动贯穿全部界面）

```
plugin.lua TOOL.icon_base64 / plugin 目录 icon.png|jpg
        │
        ▼
build_plugin_def / PluginDef ──► ToolDescriptor::icon()
        │
        ├──► ToolInfo.icon        ──►工具管理卡片 / 安装弹窗
        ├──► PluginInfo.icon     ──► 插件管理（已装 + 市场）
        ├──► ToolUsage.icon       ──► 统计（柱状条 + 树形表）
        └──► ToolEnvInfo.icon     ──► 首页（全局环境卡片）
                │
                ▼
        前端 ToolIcon.vue 统一渲染（缺省回退彩色圆点）
```

---

## 4. 兼容性 / 边界 / 风险

| 项 | 分析 | 处置 |
|---|---|---|
| 旧插件无图标 | 字段全 optional，缺省回退圆点 | 无感，无需迁移 |
| payload 膨胀 | 每个图标 base64 后约原始 1.33 倍；64px PNG 约 5-20KB | 限制 icon 文件 ≤ 512KB；可加内存缓存（按 mtime 失效） |
| 非法图片 / 损坏 base64 | 前端 `<img>` 加载失败 | ToolIcon 组件 onerror → 回退圆点 |
| base64 内容安全 | 插件可信模型已放开 os/io；data URI 仅前端渲染，不执行脚本 | 与现有插件信任模型一致，无新增风险面 |
| 禁用插件（`<name>.disabled`） | 图标读取路径需兼容禁用目录 | icon 读取按真实插件目录 + 禁用目录两处探测 |
| TOML 插件 | 可选 `icon` 字段，serde 默认缺省 | 无需强制 |

**总体风险：低。** 全部为可选字段增量，与 v2 协议（distributions/mirrors）扩展方式完全一致，有成熟先例。

---

## 5. 工作量与分阶段

| 阶段 | 内容 | 预估 |
|---|---|---|
| **P1 最小可用** | 后端：Lua `TOOL.icon_base64` + 文件 icon.png/jpg 解析、ToolInfo/PluginInfo/ToolUsage/ToolEnvInfo 透传；前端：ToolIcon 组件 + 4 主界面接入；6 个默认插件 logo 素材 | 后端 0.5-1 天 · 前端 0.5 天 |
| **P2 增强** | Lua 内置 base64 模块、插件市场 `RemotePluginInfo.icon`、插件编辑器图标上传/预览 UI、图标缓存 | 0.5-1 天 |
| 验证 | `cargo test`（解析/优先级/禁用目录用例）+ `npm run dev` 手动走查 4 界面 | — |

**验收标准**：内置 6 个工具与自定义插件在首页 /工具管理 / 插件管理 / 统计 4 处均显示对应 logo；删除图标文件或声明后自动回退彩色圆点；插件市场安装含图标的插件后图标正确展示。

---

## 6. 结论

**可行，推荐实施。** 改动集中在「插件定义解析 + 序列化模型 + 统一渲染组件」三个层面，全部向后兼容；图标以 data URI 经现有 Tauri command JSON 通道传输，前端零额外协议成本。建议按 P1 → P2 顺序推进，P1 即可覆盖用户全部诉求。

---

## 7. 实施记录（2026-08-10，P1 已落地）

> 追加需求：插件目录文件方式新增 **SVG** 支持（`icon.svg`，优先级高于 png/jpg）。

### 已实现

| 层 | 改动 |
|---|---|
| 后端 | 新增 `src-tauri/src/tool/icon.rs`（`resolve_icon`：svg > png > jpg > jpeg > 声明 base64；手写 base64 编码免依赖；文件 ≤ 512KB） |
| 后端 | `LuaPluginDef`/`PluginDef` 增加 `icon_base64`/`icon` 声明字段并解析；`ToolDescriptor` 新增 `icon_base64()` 默认方法 |
| 后端 | `ToolInfo`/`PluginInfo`/`ToolUsage`/`ToolEnvInfo` 四模型新增 `icon` 字段（data URI）并组装；禁用插件图标按原始目录（`.disabled`）解析 |
| 内置插件 | 6 个内置插件新增 `icon.svg`（**官方 logo**：Go 土拨鼠 / Rust 螃蟹 Ferris / Node.js 六边形 / Java 咖啡杯 / Python 双蛇 / Maven 徽标），`DEFAULT_PLUGIN_ICONS` 随二进制分发，`ensure_default_plugins` 首启/缺失时补写且不覆盖用户图标 |
| 前端 | 新增 `ToolIcon.vue`（有图标显示 img、加载失败/无图标回退 `DOT_COLORS` 圆点）；接入工具管理卡片、首页全局环境、插件管理（市场+已装）、统计（柱状条+树形表）、安装弹窗、项目预设选择器、项目环境行 |

### 使用方式

- **文件**：`~/.envhive/plugins/<name>/icon.svg`（或 `icon.png` / `icon.jpg`）——放文件即生效，无需改脚本；
- **Lua 声明**：插件 `工具` 表加 `icon_base64 = "iVBORw0KGgo..."`（裸 base64 或 `data:image/...;base64,...`）；
- **TOML 声明**：`icon = "..."` 同上。

### 验证

- `cargo test --lib`：48 passed / 0 failed（含 icon 解析优先级、内置图标注入与保留两个新用例）；
- `npm run build`（vue-tsc + vite build）：通过。

### 图标素材来源（2026-08-10 二次替换）

占位徽标已替换为各语言官方 logo（纯矢量 SVG，均 ≤ 512KB、无外部图片/字体引用）：

| 插件 | 图标 | 来源 |
|---|---|---|
| go | 土拨鼠 Gopher | vectorlogo.zone（golang-icon） |
| rust | 螃蟹 Ferris | rust-lang/rust-artwork（mascot/ferris-flat-noshadow.svg） |
| nodejs | 绿色 JS 六边形 | vectorlogo.zone（nodejs-icon） |
| java | 经典咖啡杯 | vectorlogo.zone（java-icon） |
| python | 蓝黄双蛇 | vectorlogo.zone（python-icon） |
| maven | Apache Maven 徽标 | vectorlogo.zone（apache_maven-icon） |

用户插件目录 `~/.envhive/plugins/*/icon.svg` 已同步；`cargo test` 内置图标注入用例（含文件解析）通过。

### 待办（P2，未实施）

- Lua 内置 `base64.encode/decode` 模块（插件内动态处理图标）；
- 插件市场 manifest `icon` 字段（`RemotePluginInfo.icon` 类型已预留）；插件编辑器图标上传/预览 UI。
