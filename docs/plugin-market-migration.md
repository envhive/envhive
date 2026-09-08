# 内置插件迁出 · 插件仓库构建方案

> 目标：把 8 个内置 Lua 插件（nodejs/java/go/rust/python/maven/tomcat/lua）从应用二进制中移出，
> 迁入独立的 **envhive-plugins** 仓库，应用不留任何内置插件，
> 全部插件经「插件市场」远程安装。首启提供引导一键安装，保证开箱体验。
>
> 决策（2026-08-13 确认）：托管平台可选 **GitHub Pages**（默认）或 **Gitee raw 直链**（国内友好，
> 见 §9）· zip 打包分发 · 首启引导一键装。

> **✅ 已落地（2026-08-14）**：采用「同仓库」变体 —— 插件不迁往独立仓库，而是留在
> **本项目仓库根 `plugins/`**（`plugins/<name>.zip` + 仓库根 `manifest.json`），
> 下载地址 = `https://raw.giteeusercontent.com/envhive/envhive/raw/main/plugins/<name>.zip`
> （备用 `https://raw.githubusercontent.com/envhive/envhive/main/...`）。
> 具体实现见 `scripts/build_plugins.py`、`plugins/README.md`、`config.rs`（默认仓库地址）、
> `extras.rs`（zip 安装链路 + 启动同步 `sync_remote_plugins`）。以下设计除仓库布局外均已按此执行。

---

## 1. 现状分析（已核对代码）

| 环节 | 现状 | 位置 |
|---|---|---|
| 内置插件源文件 | 8 个 Lua 插件，每插件仅 `plugin.lua` + `icon.svg`（无 lib/ 子目录） | `app/src-tauri/plugins/<name>/` |
| 编译进二进制 | `DEFAULT_PLUGINS` / `DEFAULT_PLUGIN_ICONS` 用 `include_str!` 内嵌脚本与图标 | `lua_plugin/def.rs:268-290` |
| 首启注入 | `ensure_default_plugins()` 写入 `~/.envhive/plugins/<name>/`（不覆盖用户文件，跳过禁用目录） | `lua_plugin/def.rs:296-326`，启动调用在 `manager/mod.rs:56` |
| 远程市场骨架 | 已有 `manifest.json` 拉取 + 远程安装 + `.market` 来源标记（builtin/market/local） | `extras.rs:201-292`、`commands/plugin.rs:80-111`，前端 `PluginsPage.vue` 市场 UI 已就绪 |
| 默认仓库地址 | `DEFAULT_REGISTRY_ADDRESS = "https://envhive.github.io/envhive-plugins"` | `config.rs:85` |
| 解压能力 | 已依赖 `zip = "2"` crate，`decompress_zip` 已实现（zip/tar.gz/tar.xz） | `Cargo.toml:30`、`tool/install.rs:83` |

### 核心缺口（本次改造的关键点）

1. **远程安装链路只支持 TOML 插件**：`extras.rs::install_remote_plugin` 下载后走 `plugin::add_plugin`（解析 `plugin.toml`）。
   8 个内置插件全是 **Lua**，需让市场支持 zip 包内的 `plugin.lua`。
2. **内置注入逻辑及其测试**需要整体移除。
3. **首启空窗**：移除内置后首次启动 `~/.envhive/plugins/` 为空，需引导安装。

---

## 2. 目标架构

```
envhive-plugins（GitHub 仓库，Pages 托管）
├── plugins/
│   ├── nodejs/{plugin.lua, icon.svg}
│   ├── java/{plugin.lua, icon.svg}
│   └── ...（8 个）
├── packages/                        # 构建产物：zip 包（commit 或 CI 生成）
│   ├── nodejs-1.0.0.zip
│   └── ...
├── manifest.json                    # 索引（schema v2）
├── scripts/
│   └── build_manifest.py            # 扫描 plugins/ → 打 zip + 算 sha256 + 生成 manifest
└── .github/workflows/pages.yml      # push → build_manifest.py → 部署 Pages

EnvHive 应用
├── 插件市场 UI（已有）→ list_remote_plugins 拉 manifest
└── install_remote_plugin → 下载 zip → sha256 校验 → 解压校验 plugin.lua
    → 原子写入 ~/.envhive/plugins/<name>/ → 写 .market
```

---

## 3. 插件仓库设计

### 3.1 仓库目录结构

```
envhive-plugins/
├── README.md                 # 仓库说明 + 插件规范 + 贡献指南
├── plugins/
│   ├── nodejs/
│   │   ├── plugin.lua        # 必需：TOOL 元信息 + available/pre_install hook
│   │   └── icon.svg          # 可选但推荐：插件图标（安装后写入 plugins/<name>/icon.svg）
│   └── lib/                  # 各插件私有模块放在各自目录的 lib/ 子目录
├── packages/                 # 构建产物：<name>-<version>.zip（gitignore 或提交均可）
├── manifest.json             # 索引（由脚本生成，勿手改）
├── scripts/
│   ├── build_manifest.py     # 见 3.3
│   └── verify_plugin.py      # 可选：静态校验 plugin.lua 必需字段
└── .github/workflows/pages.yml
```

### 3.2 zip 包规范

```
<name>-<version>.zip
├── plugin.lua               # 必需（version 取自 TOOL.version）
├── icon.svg                 # 可选，安装时提取写入
└── lib/                     # 可选，插件私有 Lua 模块（require 约定，兼容 v2 协议）
    └── *.lua
```

- zip 根目录**直接是插件文件**（不打一层 `<name>/` 目录），解压后即 `plugins/<name>/` 的镜像；
- `plugin.lua` 顶层只允许一个，缺失或校验失败即拒绝安装；
- 包内文件统一小写，禁止路径穿越（见 5.2 安全防护）。

### 3.3 manifest.json（schema v2，向后兼容）

```json
{
  "schemaVersion": 2,
  "plugins": [
    {
      "name": "nodejs",
      "version": "1.0.0",
      "type": "lua",
      "format": "zip",
      "description": "Node.js 官方发行版",
      "homepage": "https://nodejs.org",
      "downloadUrl": "https://envhive.github.io/envhive-plugins/packages/nodejs-1.0.0.zip",
      "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      "size": 4096
    }
  ]
}
```

字段说明与兼容规则：

| 字段 | 说明 | 旧客户端（schema v1） |
|---|---|---|
| `schemaVersion` | 2 | 忽略 |
| `name` / `version` / `description` / `downloadUrl` | 与 v1 一致 | 兼容 |
| `type` | `lua` \| `toml`，缺省 `toml` | 忽略（仍按 TOML 处理） |
| `format` | `zip` \| `file`，缺省 `file`（直链文本） | 忽略（仍按直链文本处理） |
| `sha256` | zip 完整性校验，`format=zip` 时必需 | 忽略（不校验） |
| `size` | 字节数（可选，供进度显示） | 忽略 |
| `homepage` | 插件主页（可选） | 忽略 |

> 兼容策略：旧版客户端读 v2 manifest 时忽略新字段，行为退化到 v1（直链 TOML）——
> 若某插件 `format=zip` 且 type=lua，旧客户端会尝试按 TOML 解析 zip 二进制而失败。
> **发布节奏建议**：仓库上线先只发布 `format=file, type=lua`（直链 plugin.lua，旧逻辑升级为 Lua 后可装），
> 宿主完成 zip 支持后再切换 `format=zip`；或在仓库上线前先发布新版宿主（推荐，本项目单机自用无此顾虑）。

### 3.4 build_manifest.py（构建脚本）

```python
# 逻辑（伪码）
for name in sorted(plugins/):
    script = read(plugins/<name>/plugin.lua)
    meta = parse_tool_table(script)          # 提取 TOOL.name / TOOL.version / TOOL.homepage
    version = meta.version or "1.0.0"
    # 1) 打 zip（幂等：内容无变化跳过）
    zpath = packages/<name>-<version>.zip
    zip(plugins/<name>/* → zpath)            # 根目录直接放文件
    # 2) 校验信息
    sha256 = sha256_hex(zpath); size = len(zpath)
    # 3) 组装 manifest 条目
    plugins.append({
        name, version, type="lua", format="zip",
        description=meta.display or name,
        homepage=meta.homepage,
        downloadUrl=f"{BASE}/packages/{zpath.name}",
        sha256, size,
    })
write_json("manifest.json", {"schemaVersion": 2, "plugins": plugins})
```

要点：
- `BASE` 在 CI 中注入 `https://envhive.github.io/envhive-plugins`（与 `config.rs:85` 默认地址一致）；
- 幂等：仅当 `plugins/<name>/` 内容变更时重打 zip，避免每次 push 都产生新 sha256；
- 校验：`plugin.lua` 缺失 / 无 `TOOL.name` / 无 `available`、`pre_install` 关键字时脚本报错退出（静态检查，宿主安装时仍会二次强校验）。

### 3.5 GitHub Actions 工作流（pages.yml）

```yaml
name: Build & Deploy
on:
  push:
    branches: [main]
  workflow_dispatch:
permissions:
  contents: read
  pages: write
  id-token: write
concurrency:
  group: pages
  cancel-in-progress: true
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with: { python-version: "3.12" }
      - run: python scripts/build_manifest.py
      - uses: actions/upload-pages-artifact@v3
        with: { path: . }          # manifest.json + packages/ 位于仓库根
      - uses: actions/deploy-pages@v4
```

> 备选：不建 Actions，直接 Settings → Pages → Deploy from branch（main/root）。
> 但 Actions 能保证 manifest 与 packages 始终同步生成，推荐使用。

---

## 4. 宿主侧改造清单（app）

### 4.1 扩展远程安装支持 Lua zip —— `extras.rs`（核心改动）

**a. `RemotePluginInfo` 扩展**（`extras.rs:202-211`）：

```rust
pub struct RemotePluginInfo {
    pub name: String,
    pub version: String,
    #[serde(default)] pub description: String,
    pub download_url: String,
    #[serde(default)] pub r#type: String,     // "lua" | "toml"，缺省 toml
    #[serde(default)] pub format: String,      // "zip" | "file"，缺省 file
    #[serde(default)] pub sha256: Option<String>,
    #[serde(default)] pub size: Option<u64>,
    #[serde(default)] pub homepage: String,
}
```

**b. `install_remote_plugin` 重写**（`extras.rs:248-292`）：

```
1. 下载 download_url → 临时文件（client 复用宿主代理）
2. 若 sha256 存在 → 计算校验，失败即拒绝（防篡改）
3. 按 format 分流：
   - file  → 沿用现有直链文本路径（type=lua 时改走 add_lua_plugin）
   - zip   → 解压到 staging 目录（复用 tool/install 的 decompress_zip）
             → 校验 plugins 根必须含 plugin.lua，且 build_plugin_def 通过
               （语法 + TOOL 元信息 + available/pre_install hook）
             → 校验通过 → 原子替换 plugins/<name>/（先写 staging，成功再 rename）
             → 从包内提取 icon.svg（存在则写，不覆盖用户已有图标）
4. 写 .market 来源标记（保持现有语义）
5. 清理临时目录
```

- 更新语义保持：同名已存在时覆盖（先移除旧 Lua 定义残留，逻辑同现有 `extras.rs:270-284`）；
- zip 解压复用 `tool::install::decompress_zip`，**无需新增 crate 依赖**（`Cargo.toml:30` 已有 zip v2）。

**c. `fetch_remote_manifest`**（`extras.rs:222-241`）：结构体扩展后 serde 自动兼容，`schemaVersion` 字段可加 `#[serde(default)]` 忽略。

### 4.2 移除内置插件 —— `lua_plugin/def.rs`

- 删除 `DEFAULT_PLUGINS`（`:268-277`）与 `DEFAULT_PLUGIN_ICONS`（`:281-290`）常量；
- 删除 `ensure_default_plugins()`（`:296-310`）与 `write_default_icon()`（`:313-326`）；
- 删除 `app/src-tauri/plugins/` 目录（8 个插件迁往新仓库）。

### 4.3 移除启动注入 —— `manager/mod.rs`

- 删除 `:56` 的 `crate::lua_plugin::ensure_default_plugins(&paths);`（含注释）。

### 4.4 测试清理 —— `lua_plugin/mod.rs`

依赖 `DEFAULT_PLUGINS` 的测试需删除或改写（测试内自带 SCRIPT 常量，不受影响）：

| 测试 | 处理 |
|---|---|
| `test_default_plugins_load`（:492） | 删除（改为从 fixture 目录加载验证，可选） |
| `test_default_icons_injected_and_preserved`（:505） | 删除 |
| `network_verify_default_plugins`（:529，ignored） | 改为读取本地 fixture 插件目录 |
| `network_verify_lua_plugin`（:552，ignored） | 改用内嵌 SCRIPT 或 fixture |
| `install_lua_binary_e2e`（:607，ignored） | 同上 |

### 4.5 命令签名（建议）—— `commands/plugin.rs:104`

`install_remote_plugin(name, download_url)` 保持不动，`sha256` 由后端在安装时从 manifest 二次获取不可行（命令不携带），
**推荐改为传整个插件对象**：

```rust
#[tauri::command]
pub async fn install_remote_plugin(
    state: State<'_, AppState>,
    plugin: crate::extras::RemotePluginInfo,   // 前端把 market 列表项整传
) -> Result<()>
```

避免前端只传 downloadUrl 导致 sha256/type 丢失、安装逻辑猜测格式。

### 4.6 前端 —— `store.ts` / `PluginsPage.vue`

- `store.ts:736` 调用改为传整个 plugin 对象；
- **首启引导**（新逻辑）：`PluginsPage.vue` 挂载后调 `list_plugins`，若为空且 `list_remote_plugins` 有结果 → 显示引导卡片：
  「未安装任何插件，是否一键安装推荐插件（nodejs/java/go/rust/python/maven/tomcat/lua）？」
  点击后顺序 await `install_remote_plugin`（或新增批量命令 `install_remote_plugins(Vec<RemotePluginInfo>)`），完成后刷新列表；
- 引导卡片只在首次展示（localStorage 标记 `envhive.bootstrap.done`）。

### 4.7 文档同步

- `README.md:22`（"内置 8 个默认 Lua 插件随应用分发"）改为描述插件市场安装流程；
- `docs/roadmap.md` 中"插件市场托管与审核"进度若标为 P3，可标注本方案落地情况。

---

## 5. 安全与兼容性

### 5.1 安全防护

| 风险 | 防护 |
|---|---|
| zip 路径穿越（zip slip） | 解压时逐条目校验：规范化后必须以目标目录为前缀、拒绝 `..` 与绝对路径（核对 `tool/install.rs:83` 现有 `decompress_zip` 是否已做，未做则补） |
| 篡改 | manifest `sha256` 校验，不匹配拒绝安装 |
| 恶意 Lua 脚本 | 沿用 v2 可信插件模型（README 已声明"仅从可信来源安装插件"）；市场仓库即可信来源，脚本内 `http.get` 仍受 NETWORK_ALLOW 白名单约束 |
| 安装中断残留 | staging 目录 + rename 原子替换，失败清理 |

### 5.2 兼容性

- **老用户**：`~/.envhive/plugins/` 已注入的 8 个插件与已安装版本目录**原样保留**，升级不删、不覆盖；`ensure_default_plugins` 移除后不再"复活"被禁用插件，符合预期；
- **manifest v1**：无 `type`/`format` 字段的旧仓库仍可拉取，按直链 TOML 处理；
- **插件的 `.market` 标记**：市场安装的插件 `source=market`，UI 已按 builtin/market/local 三态展示（`types.ts:209`），内置移除后 builtin 态自然消失，前端无硬编码依赖即可（核对 `PluginsPage.vue` 中 builtin 分支是否有空态处理）。

---

## 6. 实施步骤（建议顺序）

| # | 步骤 | 产出 | 验证 |
|---|---|---|---|
| 1 | 建 `envhive-plugins` 仓库，搬入 8 个插件目录 | 仓库源码 | `git clone` 后目录结构完整 |
| 2 | 写 `scripts/build_manifest.py` + `pages.yml`，push 部署 | 线上 `manifest.json` + `packages/*.zip` | `curl https://envhive.github.io/envhive-plugins/manifest.json` |
| 3 | 宿主改造 `extras.rs`（zip/lua 安装链路） | 代码 | `cargo build` 通过 |
| 4 | 宿主移除内置（`def.rs` / `manager/mod.rs` / 测试） | 代码 | `cargo test --lib` 全绿 |
| 5 | 前端传参 + 首启引导（`store.ts` / `PluginsPage.vue`） | 代码 | `npm run tauri dev` |
| 6 | 端到端验证 | — | 市场装 nodejs → 版本列表可拉取 → 安装 → `env_keys` 生效 |
| 7 | 文档同步（README / roadmap） | 文档 | 抽查 |

---

## 7. 端到端验证清单

```bash
# 仓库
curl -fsSL https://envhive.github.io/envhive-plugins/manifest.json | python -m json.tool
curl -fsSL -o /tmp/n.zip https://envhive.github.io/envhive-plugins/packages/nodejs-1.0.0.zip
sha256sum /tmp/n.zip   # 与 manifest 一致

# 宿主（在 app/ 下）
cargo test --lib                              # 全量单测
cargo test --lib -- --ignored network_verify_lua_plugin --nocapture   # 网络链路（手动）
npm run tauri dev                             # 手动：市场安装 → 版本列表 → 安装 → 环境变量
```

## 8. 备注

- 本方案为**设计文档**，不含代码改动；执行时按 §4 清单逐文件实施。
- zip 包内 `icon.svg` 的提取写回时机：安装成功、`plugins/<name>/icon.svg` 不存在时写入（与现 `write_default_icon` 的"不覆盖用户文件"语义一致）。
- 若未来需要多文件直链分发（不走 zip），`format=file` 保留为升级路径，`downloadUrl` 指向 `plugin.lua` 文本即可。

---

## 9. Gitee 托管变体（国内加速）

> 结论：**完全支持同一方案**，宿主代码零逻辑改动。Gitee 提供两条托管路径，推荐 **raw 直链**。

### 9.1 两条路径对比

| 维度 | GitHub Pages（默认） | Gitee Pages | Gitee raw 直链（推荐） |
|---|---|---|---|
| 国内访问 | 慢 / 不稳定 | 快 | 快 |
| 实名 / 审核 | 无 | 强制实名，**每次部署/更新都需审核**（延迟不定） | 无需（公开仓库） |
| 更新发布 | Actions push 自动部署 | 手动点「更新」，免费版每日限 10 次 | **push 即生效** |
| 默认地址形态 | `https://envhive.github.io/envhive-plugins` | `https://<user>.gitee.io/envhive-plugins` | `https://gitee.com/<user>/envhive-plugins/raw/<branch>` |
| 宿主改动 | 无 | 仅改默认地址 | 仅改默认地址（+建议调 UA） |
| 适用场景 | 海外/自动化友好 | 人看的展示页 | **机器拉 manifest/zip 的首选** |

> Gitee Pages 的「每次部署审核」与「每日 10 次更新限制」对插件仓库这类**高频自动更新**场景是硬伤；
> raw 直链绕过 Pages，纯走 Git 推送，语义与 GitHub Actions 一致（push = 发布），因此是 Gitee 上的推荐路径。

### 9.2 Gitee raw 直链落地

**仓库结构不变**（§3.1），仅两处差异：

1. **manifest 的 BASE 地址**：
   ```
   https://gitee.com/<user>/envhive-plugins/raw/master
   ```
   - 注意 Gitee 默认分支为 `master`（GitHub 是 `main`），`build_manifest.py` 的 BASE 按实际分支注入；
   - `downloadUrl` 全 URL 写法（§3.3）天然兼容，无需改动。

2. **宿主默认地址**（`config.rs:85`）：
   ```rust
   pub const DEFAULT_REGISTRY_ADDRESS: &str =
       "https://gitee.com/<user>/envhive-plugins/raw/master";
   ```
   `extras.rs` 的 `{base}/manifest.json` 拼接逻辑不变；用户也可在 config.yaml 的
   `registry.addresses` 里覆盖（前端市场地址下拉已支持多地址切换，`store.ts:882`）。

3. **UA 校验**（需验证）：Gitee raw 对非浏览器 UA 偶有拦截。宿主 client 的 UA 为
   `envhive/0.1.0`（`lib.rs:43`）。若拉取被拒（403），将 UA 改为浏览器风格
   （如 `Mozilla/5.0 (Windows NT 10.0; Win64; x64)`）即可，属一行改动。

4. **Gitee 特有限制**（不影响本方案）：
   - 公开仓库才可被无鉴权下载（方案默认公开；私有化需在 URL 带 access_token，暂不支持，列为扩展）；
   - raw 直链对超大文件/高频请求有限速，zip 包仅几 KB~几十 KB，远低于阈值。

### 9.3 双平台并行（可选）

`manifest.json` 是纯静态文件，同一仓库结构可同时推 GitHub 与 Gitee 两个远端
（`git remote add gitee ...` + 双 push），manifest 里的 `downloadUrl` 各自指向本平台地址。
用户端在 config.yaml 配置多地址即可在两边市场间切换（前端地址下拉已支持）。

### 9.4 Gitee 变体验证

```bash
# raw 直链（push 后立即生效，无审核）
curl -fsSL https://gitee.com/<user>/envhive-plugins/raw/master/manifest.json | python -m json.tool
curl -fsSL -o /tmp/n.zip <manifest 里的 downloadUrl>
sha256sum /tmp/n.zip   # 与 manifest 一致

# Gitee Pages（若采用：需实名 → 仓库「服务」→「Gitee Pages」→ 手动开启，每次更新后手动点更新）
curl -fsSL https://<user>.gitee.io/envhive-plugins/manifest.json
```
