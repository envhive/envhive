# 蜂巢 EnvHive

> 多语言运行时 · 环境配置中枢

一款桌面应用：管理 **Java / Node.js / Go / Rust** 等语言工具的安装与版本切换，并提供环境配置一键切换(切换npm仓库、cargo仓库、pip仓库、proxy等等)。

## 技术栈

| 层 | 选型 |
|---|---|
| 桌面框架 | [Tauri 2](https://tauri.app)（Rust 后端，安装包 ~10MB） |
| 前端 | React 19 + TypeScript + Vite 6 |
| 后端 | Rust（cargo，工具全部由 Lua 插件驱动，内置 `list_tools` / `switch_env` 命令） |

## 工具模型：全部插件化

蜂巢不再内置硬编码的工具注册表、也不在应用二进制中内置任何插件 —— 所有运行时
（含 Node.js / Java / Go / Rust）都由**Lua 插件**描述（`~/.envhive/plugins/<name>/plugin.lua`，
提供 `available(ctx)` / `pre_install(ctx)` / `post_install(ctx)` / `env_keys(ctx)` /
`pre_uninstall(ctx)` 生命周期 hook）。

- 插件**全部从 Git 仓库下载**：仓库根 `manifest.json`（插件市场填写的仓库地址即 manifest.json 完整地址，
  如 `https://raw.giteeusercontent.com/envhive/envhive/raw/main/manifest.json`（Gitee，默认，仓库名「官方gitee」）
  或 `https://raw.githubusercontent.com/envhive/envhive/main/manifest.json`（GitHub，备用，仓库名「官方github」）；
  zip 包位于 `plugins/<name>.zip`，manifest 内 `downloadUrl` 可为完整下载地址或相对 manifest.json
  所在目录的路径）；应用启动时后台自动同步缺失插件，也可在「插件市场」手动安装/更新；
- **插件协议**（`docs/lua-plugin-design.md`）：
  - 支持「发行商 × 版本」两级维度（`TOOL.distributions`），Java 插件含 8 个发行商；
  - 版本标识符为 SDKMAN 风格不透明字符串 `x.y.z[.fx]-<dist>`（JavaFX 直接体现在版本条目）；
  - 可信插件模型：**放开 Lua 全部标准库（含 os/io），插件可执行命令/读写文件 —— 仅从可信来源安装插件**；
  - 内置模块：`http.get`（NETWORK_ALLOW 白名单）、`json`、`archiver.extract`、`file`（限 ~/.envhive 与临时目录）、
    `versions.parse`（SDKMAN 标识符解析）；插件 `lib/` 子目录可 require 私有模块；
  - **单文件工具**：`pre_install` 返回非归档 URL（如 Lua 的预编译二进制）时按「单文件」安装
    —— 直接放置、统一命名 `<tool>[.exe]`、自动补可执行权限；`extra_files` 附带同目录依赖
    （如 Windows dll），由宿主同链路下载校验。

## 目录结构

```
envhive/
├── index.html               # 入口页
├── package.json             # 前端依赖与脚本
├── vite.config.ts           # Vite 配置（端口 1420）
├── src/                     # 前端源码（React + TS）
│   ├── main.tsx
│   ├── App.tsx              # 主界面：工具卡片 + 环境切换
│   └── styles.css
├── src-tauri/               # Rust 桌面端
│   ├── Cargo.toml
│   ├── tauri.conf.json      # 窗口 / 打包配置
│   ├── capabilities/        # 权限声明
│   ├── icons/               # 应用图标（脚本生成）
│   └── src/
│       ├── main.rs          # 桌面入口
│       ├── lib.rs           # Tauri 应用与命令
│       └── ...              # lua_plugin / plugin / manager / tool 等模块
└── scripts/
    └── gen_icons.py         # 纯 stdlib 生成蜂巢图标（PNG/ICO）
```

## 前置条件

- [Node.js](https://nodejs.org) ≥ 18
- [Rust](https://rustup.rs) stable（含 `cargo`）
- Windows：Visual Studio Build Tools（MSVC C++ 编译链，Tauri 官方要求）
  - 安装：`winget install Microsoft.VisualStudio.2022.BuildTools` 或 Visual Studio Installer 勾选「使用 C++ 的桌面开发」

## 快速开始

```bash
npm install          # 安装前端依赖
npm run tauri dev    # 启动桌面应用（自动拉起 vite + cargo）
```

仅预览前端界面（不启动桌面窗口）：

```bash
npm run dev          # 浏览器访问 http://localhost:1420
```

## 打包发布

```bash
npm run tauri build  # 产出安装包：src-tauri/target/release/bundle/
```

## 重新生成图标

```bash
python scripts/gen_icons.py
# 或使用 Tauri 官方工具从一张 1024x1024 源图生成全套：
npm run tauri icon path/to/source.png
```

## 当前进度

> 完整功能清单与里程碑见 [docs/roadmap.md](docs/roadmap.md)；Lua 插件（发行商维度）协议见 [docs/lua-plugin-design.md](docs/lua-plugin-design.md)。

- [x]工具真实安装 / 卸载与版本切换（对接各语言官方下载源，插件化驱动）
- [x] 环境变量注入（PATH / JAVA_HOME / GOROOT 等；Windows 注册表 REG_EXPAND_SZ + WM_SETTINGCHANGE）
- [x] 镜像源一键切换（npm / pip / cargo / maven / go / docker / nuget / gem / pub / conda）
- [x] 代理应用内生效（下载代理 /工具代理 / 插件请求走同一代理，不修改系统全局代理）
- [x] 下载队列（批量安装）、冲突检测、进程注入 Session、Lua 插件、远程注册表、使用统计
- [ ] 项目级配置模板（`.envhive.toml` 已支持项目目录自动定位；CLI / Shell Hook 终端注入待完成）
- [ ] P3 远期：自动更新、插件市场托管与审核、环境智能诊断
