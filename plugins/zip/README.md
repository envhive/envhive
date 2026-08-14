# EnvHive 插件仓库

本目录是 EnvHive 应用**全部插件的下载源**（不再内置任何插件到应用二进制中）。
应用启动或用户点击「插件市场」时，从下方 Git 仓库的 raw 直链拉取
`manifest.json` 与各插件 zip 包，解压安装到本地 `~/.envhive/plugins/<name>/`。

## 下载地址约定

项目提交到以下两个仓库（主 / 备）后，插件下载地址为：

| 平台 | 仓库 | manifest 地址 | 插件包地址 |
|---|---|---|---|
| Gitee（默认，国内快） | `https://gitee.com/envhive/envhive` | `https://raw.giteeusercontent.com/envhive/envhive/raw/main/manifest.json` | `https://raw.giteeusercontent.com/envhive/envhive/raw/main/plugins/<name>.zip` |
| GitHub（海外） | `https://github.com/envhive/envhive` | `https://raw.githubusercontent.com/envhive/envhive/main/manifest.json` | `https://raw.githubusercontent.com/envhive/envhive/main/plugins/<name>.zip` |

> 宿主默认配置 `app/src-tauri/crates/envhive-core/src/config.rs` 中的 `DEFAULT_REGISTRY_ADDRESS` 与上述一致；
> 用户可在「设置 → 插件仓库地址」中覆盖。

## 目录结构

```
plugins/
├── README.md            # 本说明
├── src/                 # 插件源码（可编辑的唯一事实源）
│   ├── <name>/
│   │   ├── plugin.lua   # 必需：TOOL 元信息 + available()/pre_install() hook
│   │   └── icon.svg     # 推荐：插件图标（安装后写入 plugins/<name>/icon.svg）
│   └── ...              # 8 个插件：go java lua maven nodejs python rust tomcat
├── <name>.zip           # 构建产物：zip 根目录直接放插件文件（不打一层 <name>/）
└── ...
```

仓库根 `manifest.json`：插件索引（schema v2），`downloadUrl` 为相对路径
`plugins/<name>.zip`，宿主按 `{BASE}/{downloadUrl}` 解析为完整下载地址。

## 重新构建

改完 `plugins/src/<name>/` 下的源码后，重新打 zip 并刷新 manifest：

```bash
python plugins/build_plugins.py            # 打 zip + 生成仓库根 manifest.json（含 plugins/ 兼容版）
python plugins/build_plugins.py --verify   # 仅校验 zip 与 manifest 一致性
```

校验规则（与宿主 `build_plugin_def` 对齐）：

- 每个插件必须包含 `plugin.lua`，且含 `TOOL.name/display/category/homepage/verify_bin` 字段
  （`verify_bin` 允许为空字符串 = 解压即用型工具，如 Tomcat）；
- 必须定义 `available()` 与 `pre_install()` 两个 hook；
- 打包后 `manifest.json` 会记录 `sha256` 与 `size`，宿主安装时校验完整性。

## 安全说明

- 插件为**可信来源模型**：仅从本仓库安装插件（仓库地址可在设置中修改）；
- 插件内 `http.get` 仍受 `NETWORK_ALLOW` 白名单约束；
- 安装链路对 zip 做路径穿越防护（解压条目规范化）与 `sha256` 完整性校验。
