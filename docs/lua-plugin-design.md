# envhive Lua 插件协议设计草案

> 状态：**已实现（2026-08-06）** —— 后端 `lua_plugin.rs` / `tool` / `commands` / `queue` / 前端 `App.tsx` / 内置 java 插件均按本文落地；nodejs/go/rust 插件保持 v1 结构验证兼容路径。
> 目标：支持「发行商 × 版本」多维度选择，JavaFX 作为版本级变体直接体现在版本列表中
> 前提：初步开发阶段，**不考虑旧插件/旧数据迁移**；**所有导入的 Lua 插件视为可信插件**。

---

## 1. 目标与非目标

### 目标

- **Java 类插件**：一个插件内声明多个发行商（Bisheng / Corretto / GraalVM / Temurin / Zulu …），
  前端渲染「发行商」下拉 →「版本」下拉，两级联动。
- **JavaFX 不做独立选项**：直接体现在版本条目中 —— `26.0.2-zulu`（不含 FX）与 `26.0.2.fx-zulu`（含 FX）
  是两个版本条目，用户直接在版本下拉里选（SDKMAN 同款格式）。
- **Node.js 类插件**：不声明发行商 → 前端只渲染「版本」下拉，UI 与现状一致。
- **可信插件模型**：放开 os / io 标准库，插件可执行命令、读写文件（与 vfox 对齐）。
- 版本标识符升级为 SDKMAN 风格 `x.y.z[.fx]-<dist短名>`，宿主全程当不透明字符串处理。

### 非目标

- 旧插件（v1 单发行版）迁移兼容 —— 直接演进，不保证向后兼容。
- 插件市场 / 插件签名 / 更新通道。
- 版本文件的解析（.sdkmanrc / .tool-versions 等 legacy 兼容）。

---

## 2. 插件目录结构（不变，新增可选 lib/）

```
~/.envhive/plugins/<name>/
├── plugin.lua          # 全部逻辑（工具表 + hooks），与 v1 相同入口
└── lib/                # 可选：私有模块，require("模块名") 按文件名加载
    └── versions.lua    # 例：版本标识符解析库
```

- 保持单文件 `plugin.lua` 为主入口，不拆 `hooks/` 子目录（当前规模没必要，列为未来演进项）。
- 新增 `lib/` 目录支持：宿主把 `lib/` 下所有 `.lua` 注册为可 require 模块（对齐 vfox）。
- 新增**内置模块** `versions`：提供 `versions.parse("26.0.2.fx-zulu") -> {version="26.0.2", distribution="zulu", javafx=true}`，
  插件不必自己写解析（实现参照 vfox 的 `distribution_version.lua`，扩展 `.fx` 标记）。

---

## 3.工具元数据（核心）

```lua
TOOL = {
  name = "java",
  display = "Java",
  category = "language",
  homepage = "https://adoptium.net",
  version = "1.0.0",          -- 插件自身版本（可选）：市场 manifest 有同名插件且版本不同时，插件页提供「更新」
  verify_bin = "bin/java",    -- 可空：空 = 解压即用型工具，安装后跳过运行验证（如 Tomcat）
  verify_arg = "-version",
  bin_suffix = "/bin",
  env_vars = { JAVA_HOME = "{root}" },

  -- 发行商维度（可选）：声明则前端渲染「发行商」下拉；不声明（如 nodejs）则无此维度
  distributions = {
    { key = "open",     display = "OpenJDK (Eclipse)",           default = true },
    { key = "tem",      display = "Eclipse Temurin" },
    { key = "bisheng",  display = "Bisheng (Huawei)" },
    { key = "corretto", display = "Corretto (Amazon)" },
    { key = "graal",    display = "GraalVM (Oracle)" },
    { key = "zulu",     display = "Azul Zulu" },
    { key = "albba",    display = "Dragonwell (Alibaba)" },
    { key = "librca",   display = "Liberica (BellSoft)" },
  },

  -- 缺省发行商（可选，缺省取 distributions[1]）
  default_distribution = "open",
}
```

### 字段规则

| 字段 | 类型 | 语义 |
|---|---|---|
| `distributions` | 数组（可选） | 存在 → 前端渲染发行商下拉；`default = true` 标记初始选中（缺省取第一项） |
| `default_distribution` | 字符串（可选） | 缺省发行商 key |

> JavaFX **不是**元数据维度：是否包含 JavaFX 由插件在 `available()` 里展开为独立版本条目，
> 并可用 `labels = {"jfx"}` 标记，供前端加徽标（可选，纯展示）。

### 前端渲染规则（由 Lua 声明驱动，不硬编码）

1. 有 `distributions` → 渲染「发行商」下拉；否则不渲染。
2. 「版本」下拉内容 = 选中发行商下发起的 `get_versions` 结果（JavaFX 变体已作为独立条目混在其中）。
3. JavaFX 徽标：版本条目 `labels` 含 `jfx` 时显示小徽标（如 "FX"），仅提示，不参与选择逻辑。

---

## 4. 版本标识符规范（SDKMAN 兼容）

```
完整标识符 = <version>[.fx]-<dist短名>
示例：26.0.2-zulu（不含 FX）  26.0.2.fx-zulu（含 JavaFX）
     17.0.13-tem · 17.0.13.fx-tem · 23.0.2-graal · 8.0.442-zulu
```

- **`.fx` 是版本号的一部分**（SDKMAN 同款）：`<version>.fx` 整体作为"版本+特性"段，其后 `-` 连接发行商短名。
- **由插件自拼**：`available()` / `pre_install()` 内把 `.fx` 标记与发行商短名拼进版本字符串。
- **宿主不解析语义**：完整标识符是**不透明字符串**，直接用于安装目录 `installs/<tool>/v-<id>`、配置记录
  `java = "26.0.2.fx-zulu"`、切换/卸载参数。天然隔离同版本号不同发行商/是否含 FX 的构建。
- **排序**：版本列表顺序以插件 `available()` 返回为准，前端不再重排；已安装版本置顶由前端本地处理。
  同发行商下 FX 变体建议紧邻对应非 FX 版本排列（插件输出控制）。
- **标识符与安装目录的兼容性**：`v-26.0.2.fx-zulu` 含 `.` 与 `-`，在 Windows / Linux / macOS 均为合法目录名；
  现有 `pathmeta::version_dir` 的 `v-` 前缀逻辑不变。

---

## 5. Hook 协议

### available(ctx) — 版本列表

```lua
-- ctx.distribution = 当前选中发行商 key（无选择时 = TOOL.default_distribution）
function available(ctx)
  local dist = ctx.distribution or "open"
  -- 返回该发行商的版本列表；JavaFX 变体展开为独立条目（.fx 拼在版本号段）
  return {
    { version = "26.0.2-zulu",    labels = { "stable" } },
    { version = "26.0.2.fx-zulu", labels = { "stable", "jfx" } },
    { version = "25.0.1-zulu",    labels = { "lts" } },
  }
end
```

- 不读 `ctx.distribution` 的插件（如 nodejs）行为与 v1 完全一致。
- `labels` 沿用 v1（lts / stable …），`jfx` 标记仅供前端徽标展示。

### pre_install(ctx) — 下载信息

```lua
-- ctx 新增：ctx.distribution；ctx.version 已是完整标识符
function pre_install(ctx)
  -- 可用内置模块拆回发行商/版本/fx，决定 API 参数（如 adoptium image_type=jdk|jfx）
  local p = versions.parse(ctx.version)      -- {version="26.0.2", distribution="zulu", javafx=true}
  return {
    url = "...",
    file_name = "...",
    checksum = "sha256:...",                 -- 可选
    root_hint = "jdk-26.0.2+7",              -- 可选，解压根目录
  }
end
```

### post_install(ctx) — 新增可选 hook

```lua
-- 解压完成后、链接 current 之前调用；ctx = { root, version, distribution }
-- 典型用途：macOS 上把 jdk-<v>.jdk/Contents/Home/* 上移到 root（对齐 vfox-java 的做法）
function post_install(ctx)
end
```

### ctx 注入总表

| 字段 | 说明 |
|---|---|
| `ctx.version` | 完整版本标识符（v1 已有） |
| `ctx.distribution` | 选中发行商 key（新增；无发行商维度的插件为 nil） |
| `ctx.os` / `ctx.arch` / `ctx.ext` | 平台信息（v1 已有） |

### 5.5 单文件二进制工具（Binary，新增）

部分工具（如 Lua 的预编译静态二进制）以**单个可执行文件**分发，而非 zip/tar 归档。
`pre_install` 返回的 `url`（或其路径末段）不带 `.zip` / `.tar.gz` / `.tar.xz` 扩展名时，
宿主按「单文件工具」安装：

- **安装流程**：下载 → 直接放入安装目录（`installs/<tool>/v-<ver>/`），**不解压**；
- **统一命名**：可执行文件重命名为 `<tool>`（Windows 追加 `.exe`），使 `verify_bin` /
  `env_keys` / `bin_suffix` 跨平台一致 —— 插件 `verify_bin` 写无扩展名（如 `"lua"`），
  Windows 安装验证自动探测 `.exe`；
- **权限**：非 Windows 平台自动补可执行权限（0o755）；
- **`bin_suffix` 置空**：可执行文件在安装根目录，`env_keys` 返回 `{ paths = { ctx.root } }`；
- **附加文件（`extra_files`）**：部分平台的 exe 动态依赖伴随文件（如 Windows MinGW 构建的
  `lua54.exe` 需要同目录 `lua54.dll`）——`pre_install` 可返回 `extra_files` 数组，宿主随主
  文件一并下载（同链路：镜像 + checksum + 进度事件）、校验后放入安装目录。`http.get` 返回
  UTF-8 文本，**不能**用于下载二进制，故由宿主下载器负责；
- 其余 hook（`available` / `post_install` / `env_keys`）与归档工具完全一致，
  `checksum`（`sha256:`）同样支持。

示例（Lua 插件节选）：

```lua
TOOL = {
  name = "lua",
  verify_bin = "lua",  -- 单文件统一名 <tool>[.exe]
  verify_arg = "-v",
  bin_suffix = "",
}

function pre_install(ctx)
  local extra_files = {}
  if ctx.os == "win" then
    -- Windows exe 动态依赖同目录 lua54.dll → 附加文件
    table.insert(extra_files, {
      url = "https://github.com/.../lua54.dll",
      file_name = "lua54.dll",
      checksum = "sha256:...",
    })
  end
  return { url = "https://github.com/.../lua54", file_name = "lua54",
           checksum = "sha256:...", extra_files = extra_files }
end

function env_keys(ctx)
  return { paths = { ctx.root } }
end
```

---

## 6. 可信插件模型（安全策略调整）

> 决策：**所有导入的 Lua 插件视为可信插件**，不做恶意代码防线，只保留防误伤纵深。

| 项 | v1（现状） | v2（可信） |
|---|---|---|
| Lua 标准库 | 仅 COROUTINE/TABLE/STRING/UTF8/MATH（禁 os/io/debug） | **放开全部**，含 IO/OS（可 `os.execute` / `io.popen` / `print`，对齐 vfox） |
| `http.get` | NETWORK_ALLOW 声明了才收紧 | 保持（缺省未声明 = 放行；声明了仍收紧，防插件误打野域名） |
| `file` 模块 | 仅 ~/.envhive 与临时目录 | 保持路径限制（防插件误写系统文件，属防误伤） |
| 新增能力 | — | 内置 `versions.parse()` 解析库；`http.head()` 返回状态码（轻量探测资源存在性，如 dlcdn 是否保留旧版本） |

- 风险声明：可信模型下插件可执行任意命令，**只应从可信来源安装插件**；UI 的「添加插件」处加一句提示。

---

## 7. 后端改动点（Rust）

| 位置 | 改动 |
|---|---|
| `lua_plugin.rs` `LuaPluginDef` | 新增 `distributions: Vec<DistributionInfo>`、`default_distribution: Option<String>`；解析工具表新字段（`DistributionInfo { key, display }`） |
| `lua_plugin.rs` `hook_available` | 签名改 `hook_available(def, distribution: Option<&str>)`，ctx 注入 `distribution` |
| `lua_plugin.rs` `resolve_lua_package` | ctx 注入 `distribution`；新增 `hook_post_install` 调度 |
| `lua_plugin.rs` 沙箱 | `safe_stdlib()` 放开 IO/OS；`inject_modules` 新增 `versions` 解析模块（拆分 `<version>[.fx]-<dist>`，规则与插件自拼保持一致） |
| `tool/runtime.rs` `AvailableVersion` | 不变（version/lts/labels 已够用） |
| `tool/mod.rs` `ToolInfo` | 新增 `distributions: Vec<DistributionInfo>`、`defaultDistribution: Option<String>`（加 `Serialize`） |
| `pathmeta.rs` `versions_cache_file` | key 支持发行商：`<tool>-<distribution>.json`（无发行商时保持 `<tool>.json`） |
| `commands/mod.rs` `get_versions` | 签名加 `distribution: Option<String>`；透传给 `hook_available`；缓存 key 按发行商 |
| `tool/install.rs` | 解压完成后（`current` 链接前）调用 `hook_post_install`（若插件声明） |

---

## 8. 前端改动点（src/App.tsx）

| 位置 | 改动 |
|---|---|
| 类型 | `ToolInfo` 增加 `distributions` / `defaultDistribution` |
| 状态 | `selections[tool]` 由 `string` 变为 `{ dist?: string; version: string }`；`versionsMap` key 变为 `${tool}-${dist}` |
| `loadVersions` | 携带 `distribution` 调 `get_versions`；发行商切换 → 重新拉取并清空版本选择 |
| ToolCard 渲染 | ① 有 `distributions` → 「发行商」下拉（初始 = `defaultDistribution`）② 「版本」下拉（内容来自联动后的 `versionsMap`）；版本条目 labels 含 `jfx` 时显示 "FX" 小徽标 |
| 排序 | 版本列表不再 sort（信任插件顺序）；已安装置顶仍由前端做 |
| 添加插件页 | 加「仅从可信来源安装插件」提示文案 |

---

## 9. 示例插件

### 9.1 java（多发行商，协议 v2）

```lua
TOOL = {
  name = "java",
  display = "Java",
  category = "language",
  homepage = "https://adoptium.net",
  verify_bin = "bin/java",
  verify_arg = "-version",
  bin_suffix = "/bin",
  env_vars = { JAVA_HOME = "{root}" },

  distributions = {
    { key = "open",     display = "OpenJDK (Eclipse)",           default = true },
    { key = "tem",      display = "Eclipse Temurin" },
    { key = "bisheng",  display = "Bisheng (Huawei)" },
    { key = "corretto", display = "Corretto (Amazon)" },
    { key = "graal",    display = "GraalVM (Oracle)" },
    { key = "zulu",     display = "Azul Zulu" },
    { key = "albba",    display = "Dragonwell (Alibaba)" },
    { key = "librca",   display = "Liberica (BellSoft)" },
  },
  default_distribution = "open",
}

NETWORK_ALLOW = { "api.adoptium.net", "api.foojay.io" }

-- available：按 ctx.distribution 选 API/参数，版本号拼上 "[.fx]-<短名>"
function available(ctx)
  local dist = ctx.distribution or "open"
  -- 简化示意：返回该发行商若干版本，含 FX 变体
  return {
    { version = "26.0.2-" .. dist,      labels = { "stable" } },
    { version = "26.0.2.fx-" .. dist,   labels = { "stable", "jfx" } },
    { version = "17.0.13-" .. dist,     labels = { "lts" } },
    { version = "17.0.13.fx-" .. dist,  labels = { "lts", "jfx" } },
  }
end

function pre_install(ctx)
  local p = versions.parse(ctx.version)
  -- p = { version="26.0.2", distribution="zulu", javafx=true }
  -- 按 p.distribution + p.javafx 决定 API（adoptium image_type=jdk|jfx / foojay / azul / github…）
  return { url = "...", file_name = "...", checksum = "sha256:...", root_hint = "jdk-26.0.2+7" }
end

function post_install(ctx)
  if ctx.os == "darwin" then
    -- 整理 macOS .jdk 目录（可信模型，可直接 os.execute）
  end
end
```

### 9.2 nodejs（无发行商，协议 v1 结构不变）

```lua
TOOL = {
  name = "nodejs",
  display = "Node.js",
  category = "language",
  homepage = "https://nodejs.org",
  verify_bin = "node",
  verify_arg = "--version",
  bin_suffix = "",
  env_vars = {},
}
-- 无 distributions → 前端不渲染发行商下拉，仅版本下拉
function available()
  return { { version = "22.14.0", labels = { "lts" } }, { version = "24.0.0", labels = { "stable" } } }
end
```

---

## 10. 实施顺序

1. **Rust 模型**：`LuaPluginDef` 解析 `distributions / default_distribution`；`ToolInfo` 透出（前端可先看到字段）。
2. **Rust hook 链路**：`hook_available` 注入 `ctx.distribution`；版本缓存 key 按发行商拆分；`get_versions` 命令签名扩展。
3. **可信沙箱**：`safe_stdlib()` 放开 IO/OS；注入内置 `versions.parse` 模块；新增 `post_install` hook 调度。
4. **插件**：java 插件改为多发行商实现（含 `.fx` 变体）；nodejs 插件不动，验证 v1 兼容路径。
5. **前端联动**：两级选择（发行商 → 版本），切换发行商时重拉版本并清空版本选择；`jfx` 徽标。
6. **验证**：java 全流程（安装 zulu / zulu+FX / temurin / graal / bisheng、切换、卸载、环境变量注入）+ nodejs 回归。

---

## 附：与 vfox 的对应关系（实现时可对照参考）

| envhive v2 概念 | vfox 对应 |
|---|---|
| `TOOL.distributions` | vfox-java 的短名表 + metadata notes（vfox 靠插件内部约定，envhive 结构化声明） |
| 版本内 `.fx` 标记（`26.0.2.fx-zulu`） | vfox-java 的 `-fx` 后缀（`17.0.13-tem-fx`）；envhive 采用 SDKMAN 的 `.fx` 位置，位于版本号段 |
| `versions.parse()` 内置模块 | `lib/distribution_version.lua`（envhive 版本解析 `.fx` 标记） |
| `available(ctx.distribution)` | `Available(ctx)` 读 `ctx.args[1]` |
| `post_install` | `PLUGIN:PostInstall(ctx)`（mac .jdk 整理） |
| 可信沙箱（放开 os/io） | vfox gopher-lua 全库放开 |
