# 蜂巢 EnvHive — 开发路线图（Roadmap）

> 参考 [vfox](https://github.com/version-fox/vfox) 的多语言版本管理模型，结合桌面应用特性设计。
> 蜂巢定位：**多语言运行时 · 环境配置中枢** —— 不只是装工具、切版本，更是整套开发环境的配置中枢（镜像源 / 代理 / 环境变量）。
>
> 本文件 = **功能清单**（按优先级分组，每项带档位标签）+ **后端技术实现**（模块级设计，参照 vfox）+ **版本里程碑**（每档的交付边界与验收标准）。

---

## 一、优先级体系（六档细化）

| 档位 | 含义 | 目标版本 | 判断标准 |
|---|---|---|---|
| **P0 · 地基** | 架构与基础设施 | v0.1（MVP） | 缺了它，其他功能无法实现 |
| **P0 · 核心** | MVP 功能闭环 | v0.1（MVP） | 缺了它，产品不成立（装 / 切 / 删 / 查） |
| **P1 · 体验** | 完整度与易用性 | v1.0 | 缺了它能用但难用、不放心 |
| **P1 · 开放** | 扩展与生态能力 | v1.0 | 缺了它封闭但可用 |
| **P2 · 增殖** | 增强功能 | v1.1 | 锦上添花 |
| **P3 · 远期** | 探索性 / 依赖生态成熟 | v1.2+ | 时机未到，先占位 |

**使用约定**：
- 每个功能项以 `[档位]` 标签标注；子项继承父项档位，有差异时单独标注；
- 依赖关系在括号内标注 `(依赖: X)`，指向其他功能项或后端模块；
- 勾选状态：`[ ]` 待办 / `[x]` 已完成。

---

# 第一部分：功能清单（按优先级分组）

## 二、P0 · 地基（v0.1 前提）

> 架构骨架，本档完成即具备开发所有功能的基础。

- [x] **[P0·地基] Tauri Command 层与统一错误模型**：所有 `#[tauri::command]` 接口（`install_tool` / `list_tools` / `switch_version` / `set_env` …），统一返回 `Result<T, EnvHiveError>`（结构化错误码，见模块 K）
- [x] **[P0·地基] 全局配置 `config.yaml`**：`~/.envhive/config.yaml`，serde 反序列化 + 默认值 merge + 启动时写权限校验
- [x] **[P0·地基] 日志系统**：`tracing` 分级落盘到 `~/.envhive/logs/`，前端可导出调试日志
- [x] **[P0·地基] 事件总线**：Tauri `emit` 向前端推送下载进度、错误通知、状态变更
- [x] **[P0·地基] 路径元数据与目录初始化**：首次运行创建 `~/.envhive/` 完整目录布局（见模块 C），无网络依赖
- [x] **[P0·地基] 三作用域配置链模型**：`Global / Project / Session` + `Envs` 合并语义（PATH 前置、Vars 后覆盖，见模块 B）
- [x] **[P0·地基] 跨平台抽象骨架**：`env/` 模块接口定型（symlink/junction、Windows 注册表、shell profile），Unix/Windows 各留实现桩

## 三、P0 · 核心（v0.1 MVP 闭环）

> MVP 用户故事：**安装工具→ 全局切换版本 → 新终端生效 → 卸载**。全部落在单机单用户。

###工具管理

- [x] **[P0·核心]工具注册表**：内置 4 个工具定义（Java/OpenJDK、Node.js、Go、Rust）
- [x] **[P0·核心]工具发现源配置**：每种工具声明版本列表 + 下载包获取方式
  - GitHub Releases API（Go / Rust / Bun）—— 通用实现保留，Go 改走官方 `go.dev/dl`、Rust 改走 `channel-rust-stable.toml`（免限流）
  - nodejs.org dist API（Node.js）
  - Adoptium API（Java / OpenJDK）
  - 静态文件索引（Python / Zig / Deno）—— 解析器就绪，内置工具暂未使用
- [x] **[P0·核心]工具CRUD 接口**：`add_tool` / `remove_tool` / `list_tools` / `get_tool(name)`（UI 层直接消费；`list_tools`/`get_tool` 已实现，`add/remove` 依赖 P1 TOML 插件机制）

### 版本发现

- [x] **[P0·核心] 版本列表获取**：从工具源拉取可用版本，解析、排序（`VersionProvider` trait）
- [x] **[P0·核心] Semver 解析与排序**：正确排序 `1.9` / `1.10` / `2.0-rc1`，兼容非标准版本（`2024.10.12`）
- [x] **[P0·核心] 模糊匹配**：精确 → 前缀（`21` → `21.5.1`）→ 标签（`latest` / `lts` / `stable`）
- [x] **[P0·核心] 版本列表缓存**：`~/.envhive/installs/versions/<tool>.json`，默认 TTL 12h 可配
- [x] **[P0·核心] 搜索 / 过滤**：`search_sdk(name, filter)`，按版本号与标签过滤

### 下载与安装

- [x] **[P0·核心] 平台检测**：`std::env::consts::OS` + `ARCH` → 匹配正确二进制包（含平台别名映射，如 win-x64）
- [x] **[P0·核心] 异步下载**：`reqwest` + `tokio`，支持断点续传（HTTP Range）与代理
- [x] **[P0·核心] 进度回调**：`emit("download-progress", {tool, version, percent, speed, stage})` 分阶段推送
- [x] **[P0·核心] 完整性校验**：SHA256 / SHA512 / SHA1 / MD5 多算法（对照官方 checksum；`"none"` 跳过）
- [x] **[P0·核心] 解压与安装**：`.zip` / `.tar.gz` / `.tar.xz` → `~/.envhive/installs/<tool>/v-<version>/`（原子写入，见模块 E；`.7z` 暂返回 `UNSUPPORTED`，P1 引入 sevenz-rust）
- [x] **[P0·核心] 安装后验证**：运行 `java -version` / `node -v` / `go version` 确认可用，失败自动清理并报错
- [x] **[P0·核心] 卸载**：删除版本目录 + 移除对应 symlink + 更新已安装清单

### 版本切换（全局）

- [x] **[P0·核心] 全局切换**：写入 Global scope 的 `.envhive.toml` + 重建 `~/.envhive/tools/<tool>/current` 链接
  - Windows：用户级 PATH 写注册表 `HKCU\Environment`（REG_EXPAND_SZ + WM_SETTINGCHANGE 广播）
  - macOS / Linux：符号链接 + shell profile 注入（链接已实现；profile 注入归入 P1 Shell Hook）
- [x] **[P0·核心] 当前版本查询**：`current_tool(name)` / `list_installed()`（UI 首页展示）

## 四、P1 · 体验（v1.0）

> 让产品从"能用"到"好用、敢用"：进度可见、冲突有提示、环境一键到位。

- [x] **[P1·体验] 下载队列**：批量安装多个工具，前端队列列表 + 各自独立进度
- [x] **[P1·体验] 新版本通知**：后台定时刷新版本缓存（每 30 分钟），diff 出新增版本 → 前端提醒（`new-versions` 事件 + 通知条 + 系统通知；手动"检查更新"按钮）
- [x] **[P1·体验] 冲突检测**：同一工具尝试设置不同版本 / 目标配置文件被用户手动修改（mtime + hash 指纹，`~/.envhive/state/fingerprints.json`）时提示（`check_conflicts` + "确认基线"解除）
- [x] **[P1·体验] 环境变量注入**：切换工具时设置 `JAVA_HOME` / `GOROOT` / `NODE_PATH` / `CARGO_HOME` 等（依赖: Envs 合并）
- [x] **[P1·体验] 环境导入 / 导出**：当前环境（工具版本 + 镜像 + 代理 + 环境变量）YAML/JSON 导入导出（`export_env` / `import_env`）
- [x] **[P1·体验] 镜像源与仓库配置**：一键切换 npm / pip / cargo / maven / go 镜像（预设 + 写入 + 验证 + 备份还原，见模块 I）
- [x] **[P1·体验] proxy 统一管理**：代理总开关，两层联动（蜂巢下载代理 /工具代理），NO_PROXY 白名单；仅应用内生效，不写系统全局环境变量（见模块 I）
- [x] **[P1·体验] 代理可用性探测**：切换时对目标源做 HEAD 请求，代理不通提示回退（`proxy::probe` 已实现）
- [x] **[P1·体验] 系统托盘**：托盘图标 + 右键菜单（打开主界面 / 退出，左键单击显示窗口）
- [x] **[P1·体验] 通知提醒**：下载完成、新版本可用、切换成功、校验失败（前端 toast + 系统通知 Web Notification API）
- [x] **[P1·体验] 进程注入 Session**：桌面 UI"以此环境启动"（终端 / IDE / 开发服务器），子进程继承环境即会话（见模块 G）
- [x] **[P1·体验] 批量操作**：UI 批量勾选工具一键安装 / 一键切换（下载队列支持多任务入队）
- [x] **[P1·体验] 离线模式**：已下载版本离线可用，不依赖网络（缓存优先）

## 五、P1 · 开放（v1.0）

> 让产品从"封闭"到"可扩展"：自定义工具源、终端 hook、会话能力开放。

- [x] **[P1·开放] 声明式 TOML 插件机制**：工具源 = 一段 TOML（版本列表 URL + 包模板 + 平台映射），安全无需沙箱（见模块 H；`~/.envhive/plugins/<name>/plugin.toml`，`add_tool`/`remove_tool`）
- [x] **[P1·开放] 远程注册表**：`registry.address/manifest.json` 拉取插件 manifest → 下载安装（`list_remote_plugins` / `install_remote_plugin`）
- [ ] **[P1·开放] Shell Hook 终端注入**：`envhive activate` + `envhive env -s <shell>`，终端内 `cd` 自动生效（见模块 G）
- [x] **[P1·开放] 应用内 Session 预览**：暂存 Envs 不落盘，可预览 env 差异、导出脚本，一键"升级"为 Global / Project（`preview_env` 输出 PATH 片段 + export/unset 脚本 + `spawn_with_env` 进程注入）
- [x] **[P1·开放] 自定义工具源**：用户本地 TOML 定义私有工具（内网分发、公司内部工具链）

## 六、P2 · 增殖（v1.1）

> 锦上添花，但不影响核心使用。

- [x] **[P2] Lua 插件支持（v2 多发行商协议）**：`mlua` 完整脚本能力（对应 vfox gopher-lua，http/json/archiver/file/versions 模块 + available(ctx)/pre_install(ctx)/post_install(ctx)/env_keys(ctx)/pre_uninstall(ctx) 生命周期 hook；`~/.envhive/plugins/<name>/plugin.lua` + 可选 lib/ 私有模块目录）。**v2 可信插件模型**：放开 os/io 全部标准库（可执行命令，仅从可信来源安装），网络白名单 NETWORK_ALLOW 与 file 模块路径限制保留；发行商维度（TOOL.distributions）+ SDKMAN 风格版本标识符（`x.y.z[.fx]-<dist>`，JavaFX 直接体现在版本条目）
- [x] **[P2] 镜像预设扩展**：Docker Hub（~/.docker/config.json registry-mirrors）/ NuGet（NuGet.Config packageSources）/ RubyGems（~/.gemrc :sources:）/ pub.dev（~/.pub-cache/config.json PUB_HOSTED_URL）/ conda（~/.condarc channels）
- [x] **[P2] 系统代理联动**：跟随操作系统代理设置（Windows `HKCU\...\Internet Settings` ProxyEnable/ProxyServer / macOS scutil / GNOME gsettings），一键套用到蜂巢三层（下载代理 +工具代理 + 环境变量）
- [x] **[P2] 开机自启动（可选）**：常驻托盘（Windows Run 键 / Linux autostart desktop / macOS LaunchAgent），`--autostart` 启动隐藏主窗口
- [x] **[P2] 下载加速镜像**：蜂巢自身工具下载的国内加速源（nodejs→npmmirror、go→阿里云、rust→rsproxy），前缀替换规则表，用户可自定义，作用于版本列表 / 下载包 / checksum 全链路
- [x] **[P2] 使用统计**：`~/.envhive/state/usage.json` 记录各工具版本使用频率（安装/切换时）与磁盘占用，辅助释放磁盘（按闲置天数清理，保护当前版本）

## 七、P3 · 远期（v1.2+）

> 探索性，占位跟踪。

- [ ] **[P3] 自动更新**：工具新版本 → 自动下载 → 通知切换（可选全自动）
- [ ] **[P3] 插件市场托管与审核**：官方插件市场、版本兼容性校验、安全审核流程
- [ ] **[P3] 环境智能诊断**：项目缺工具/ 版本不匹配 / 镜像失效时的自动诊断与修复建议

---

# 第二部分：后端技术实现（参照 vfox）

> 直接借鉴 vfox 的 Go 后端架构（`internal/{tool,env,plugin,shell,config,pathmeta}`），映射到 envhive 的 Rust/Tauri 技术栈。
> 核心思路：**真实版本平铺存储 + 符号链接切换 + 三作用域配置链（global/project/session）**。
> 每章标注对应功能档位，便于按档开发时定位设计文档。

## A. Rust 模块架构（[P0·地基]）

```
src-tauri/src/
├── main.rs            # 桌面入口
├── lib.rs             # Tauri Builder + #[tauri::command] 注册
├── manager.rs         # EnvHiveManager —— 对应 vfox Manager，工具实例缓存（RWMutex）
├── tool/               # 对应 vfox internal/tool —— Tool trait 与 impl
│   ├── mod.rs         #   Tool trait 定义、impl 结构体
│   ├── runtime.rs     #   Runtime / RuntimePackage / Version 类型
│   ├── resolver.rs    #   版本解析（精确 → 前缀 → 模糊 + semver 排序）
│   └── download.rs    #   下载管道：Download → Checksum → Decompress
├── env/               # 对应 vfox internal/env —— 作用域与环境变量
│   ├── mod.rs         #   UseScope 枚举（Global/Project/Session）
│   ├── vars.rs        #   Envs { vars: HashMap, paths: SortedSet }
│   ├── registry_windows.rs  # Windows 注册表 PATH 操作
│   └── symlink.rs     #   Unix symlink / Windows junction 统一抽象
├── config.rs          # 对应 vfox internal/config —— 全局 config.yaml（serde）
├── pathmeta.rs        # 对应 vfox internal/pathmeta —— 路径元数据
├── plugin/            # 对应 vfox internal/plugin ——工具源 / 扩展插件
├── shell/             # 对应 vfox internal/shell —— hook 脚本生成（[P1·开放]）
├── registry/          # 镜像源管理：npm / pip / cargo / maven / go / proxy（[P1·体验]）
├── toml_chain.rs      # 对应 vfox env/vfox_toml_chain —— 三 scope 配置链合并
└── util/              # 版本比较、checksum、解压、进程探测
```

**核心：Tool trait**（对应 vfox `internal/tool/tool.go`）：

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    async fn install(&self, version: &Version) -> Result<()>;
    async fn uninstall(&self, version: &Version) -> Result<()>;
    async fn available(&self, args: &[String]) -> Result<Vec<AvailableRuntimePackage>>;
    fn env_keys(&self, pkg: &RuntimePackage) -> Result<Envs>;
    async fn use_with_config(&self, version: Version, scope: UseScope, unlink: bool) -> Result<()>;
    fn unuse(&self, scopes: &[UseScope]) -> Result<()>;
    fn get_runtime_package(&self, version: &Version) -> Result<RuntimePackage>;
    fn check_runtime_exist(&self, version: &Version) -> bool;
    fn installed_list(&self) -> Vec<Version>;
    fn current(&self) -> Version;
    fn metadata(&self) -> &Metadata;
    /// 为指定版本在指定 scope 下创建 symlink / junction
    fn create_symlinks_for_scope(&self, version: &Version, scope: UseScope) -> Result<()>;
    /// 返回指向 symlink 目录的环境变量（PATH 指向链接而非真实目录）
    fn env_keys_for_scope(&self, version: &Version, scope: UseScope) -> Result<Envs>;
}
```

**Manager：工具实例缓存**（对应 vfox `Manager{openSdks map[string]tool.Tool; mu sync.RWMutex}`）：

```rust
pub struct EnvHiveManager {
    runtime_env_ctx: Arc<RuntimeEnvContext>,
    open_tools: Mutex<HashMap<String, Arc<dyn Tool>>>,  // name(小写) -> Tool 实例
}

impl EnvHiveManager {
    pub fn lookup_tool(&self, name: &str) -> Result<Arc<dyn Tool>>;         // 缓存 → plugins/<name> 加载
    pub fn lookup_sdk_with_install(&self, name: &str) -> Result<Arc<dyn Tool>>; // 未装则经 registry 自动 add
    pub fn resolve_version(&self, sdk_name: &str, version: Version) -> Result<Version>;
    pub fn clean_tmp(&self) -> Result<()>;  // 每日清理 session 临时目录（PID 已死则删除）
}
```

## B. 三作用域配置链（[P0·地基]）—— 核心数据模型

```rust
#[derive(Clone, Copy, PartialEq)]
pub enum UseScope { Global, Project, Session }
```

- **配置文件位置**（对应 vfox `.vfox.toml`）：
  - Global：`~/.envhive/.envhive.toml`
  - Project：`<project>/.envhive.toml`（由当前工作目录向上定位）
  - Session：`~/.envhive/tmp/<YYYYMMDD>-<pid>/config.toml`（进程退出即失效；pid 为被注入进程）
- **配置链合并**：`Global < Session < Project`（后者优先），tail 优先逐层 merge；
- **文件格式采用 TOML**（与 vfox 生态对齐、内联表表达多属性、注释友好）：

```toml
[tools]
nodejs = "22.11.0"
java = { version = "21", vendor = "openjdk" }
golang = { version = "1.24.1", unlink = "true" }  # 项目级禁用链接，降级为 session 生效

[env]                      # 蜂巢扩展：项目级环境变量
DEBUG = "true"
NODE_ENV = "development"
```

- **弹性反序列化**（`#[serde(untagged)]`，对应 vfox `ToolConfig.UnmarshalTOML`）：

```rust
#[derive(Deserialize)]
#[serde(untagged)]
pub enum ToolValue {
    Plain(String),
    Attrs { version: String, vendor: Option<String>, unlink: Option<bool> },
}
```

**Envs 合并语义**（对应 vfox `internal/env/env.go`）：
- **PATH（Paths）**：高优先级 scope 在前（前置插入），项目版本优先于全局；
- **Vars（环境变量）**：低优先级先并、高优先级后覆盖（后者胜出）；
- **保留用户注入**：`SplitSystemPaths()` 保留 envhive 路径之前的用户注入段（如 virtualenv），避免误删。

```rust
#[derive(Default)]
pub struct Envs {
    pub vars: HashMap<String, Option<String>>,  // None 表示 unset
    pub paths: SortedSet<String>,               // 去重且保序
}
pub fn merge_by_scope_priority(envs_list: &[Envs]) -> Envs;
```

## C. 存储模型与版本切换（[P0·核心]）—— vfox 的精髓

### 目录布局（对应 vfox `internal/pathmeta`）

```
~/.envhive/
├── config.yaml              # 全局配置（代理 / 存储路径 / 注册表地址 / 缓存 TTL）
├── installs/                   # Shared 根（真实安装物都在这，平铺不可变）
│   └── <tool>/               # 如 nodejs/
│       ├── v-21.5.1/        #   v- 前缀：主包安装目录
│       │   └── node-v21.5.1-win-x64/   # <name>-<version> 真实目录
│       └── add-xxx/         #   add- 前缀：附加包（如 npm 内置但独立）
├── plugins/<tool>/           #工具源插件目录（TOML / Lua）
├── tools/                    # Global scope 的 symlink 目录（current -> cache 中某版本）
├── tmp/                     # Session scope 临时目录 + 会话配置
└── installs/versions/<tool>.json  # 版本列表缓存
```

### 切换 = 改配置 + 重建符号链接（不做任何复制）

- 真实版本永远存于 `installs/<tool>/v-<version>/`，只读不删（除卸载）；
- 每个 scope 有链接目录，PATH 只指向链接：Global `~/.envhive/tools/<tool>/current`、Project `<project>/.envhive/tools/<tool>/current`、Session `~/.envhive/tmp/<session>/tools/<tool>/current`；
- 切换操作：写 scope TOML + 重建 symlink → 秒级生效；
- **跨平台链接**：Unix `std::os::unix::fs::symlink`；Windows 用 **junction（reparse point）而非符号链接** —— 创建无需管理员权限（`windows-sys` 的 `NtCreateFile` / `FSCTL_SET_REPARSE_POINT`，或 `junction` crate），避免 UAC 弹窗。

### Windows 注册表 PATH 操作（对应 vfox `registry_windows.go`）

- 目标：`HKCU\Environment`（用户级，无需管理员）；
- **写 PATH 用 `SetExpandStringValue`（REG_EXPAND_SZ）**，保持 `%VAR%` 展开形式，避免变量被字面化；
- 修改后 **`SendMessageTimeoutW(WM_SETTINGCHANGE)` 广播**，新进程立即感知（无需重启/注销）；
- 增删路径**大小写不敏感去重**，避免 `C:\Foo` 与 `c:\foo` 重复。

### Shell 环境变量注入（全局模式）

- Windows 全局：写注册表 + WM_SETTINGCHANGE 广播；
- Unix 全局：桌面端写入 shell profile 的 `envhive env --global` 片段（只写一次，桌面端管理）。

## D. 下载、校验与解压管道（[P0·核心]）

对应 vfox 安装流程 `Download → Checksum.Verify → Decompress`：

```rust
pub async fn install_pipeline(tool: &dyn Tool, pkg: &RuntimePackage) -> Result<()> {
    let tmp = temp_dir()?;
    let file = download(&pkg.url, tmp).await?;          // reqwest + Range 断点续传
    checksum::verify(&file, &pkg.checksum)?;            // 多算法比对
    decompress(&file, &pkg.install_dir)?;               // zip/tar.gz/tar.xz/7z
    run_install_hooks(pkg)?;                            // 插件 PostInstall
    verify_install(&pkg.executable)?;                   // 运行 java -version 等
    persist_record(&pkg)?;                              // 写入已安装清单
    Ok(())
}
```

- **校验**：`ChecksumItem { sha256, sha512, sha1, md5 }` 多算法并存，读全文算 hash 比对；`"none"` 跳过；
- **解压**：`zip` + `tar`/`flate2` + `xz2` + `sevenz-rust`（对应 vfox `klauspost/compress` + `ulikunitz/xz` + `bodgit/sevenzip`）；
- **进度**：reqwest 流式读取 → `emit("download-progress", ...)`；断点续传：临时文件记录字节数，中断后 `Range: bytes=N-` 续传；
- **失败清理**：失败即删临时文件；安装目录先写 `.envhive.tmp` 再原子改名，避免脏目录。

## E. 版本发现与缓存（[P0·核心]）

- **统一 `VersionProvider` trait**：GitHub Releases / nodejs.org / Adoptium / 静态索引四种实现；
- **排序**：点分数字逐段比较（`1.9 < 1.10 < 2.0-rc1`），`semver` crate 为主 + 自定义比较兜底（适配 `2024.10.12` 等）；
- **模糊匹配**（vfox `preUse`）：精确 → 前缀（`21` → `21.5.1`）→ 标签（`latest` / `lts` / `stable`）；
- **缓存 TTL**：默认 12h（vfox `cache.availableHookDuration`，-1 永不过期、0 禁用），config.yaml 可配。

## F. Shell Hook 与 Session 落地（[P1·开放] / [P1·体验]）—— direnv 式方案

> vfox 的 Session 依赖 CLI `eval` 注入；桌面程序无"当前 shell"，故 Session 以**进程注入模式**为主、CLI 为辅，共享同一份 `.envhive.toml`。

### 桌面端 Session 的三层落地（进程注入优先）

| 模式 | 实现 | 场景 |
|---|---|---|
| **① 进程注入（[P1·体验]）** | 桌面 UI"以此环境启动"：合并完整 Envs → 以定制 env 拉起子进程（终端 / IDE / 服务器）→ 子进程及后代继承 = 一个会话 | 桌面日常：点一下"用 Node 21 打开终端" |
| **② CLI 辅助（[P1·开放]）** | `envhive activate` + `envhive env -s <shell>` 输出 export 语句，终端手动 eval | 已有终端会话的命令行用户 |
| **③ 应用内预览（[P1·开放]）** | 暂存 Envs 不落盘：预览 env 差异、导出脚本，一键"升级"为 Global / Project | UI 预览、团队分享前检查 |

```rust
// 进程注入：子进程环境 = 链上合并结果 + Session 覆盖
pub async fn spawn_with_env(
    manager: &EnvHiveManager,
    project: Option<&Path>,
    command: &str, args: &[&str],
) -> Result<Child> {
    let envs = manager.resolve_envs(project)?;   // Global + Project (+ Session) 链式合并
    Command::new(command).args(args)
        .envs(envs.as_map())     // 含 PATH 前置、JAVA_HOME、镜像代理变量等
        .current_dir(project)
        .spawn()
}
```

- **会话目录**：`~/.envhive/tmp/<YYYYMMDD>-<pid>/`，被注入进程退出后由 `clean_tmp()` 每日清理（PID 已死即删）；
- **升级语义**：Session 预览 → "保存为项目配置" → 写 Project/Global，Session 层删除，实现"先试后定"。

### CLI 双命令配合（[P1·开放]）

| 命令 | 职责 |
|---|---|
| `envhive activate` | 输出 shell 脚本，安装 hook 到当前 shell |
| `envhive env -s <shell>` | 计算当前目录 env 差异，输出 `export` / `unset` 语句 |

```bash
export PATH="/path/to/.envhive/tools/nodejs/current/bin:$PATH"
export JAVA_HOME="/path/to/.envhive/tools/java/current"
unset NODE_OPTIONS
```

| Shell | Hook 机制 |
|---|---|
| bash | `PROMPT_COMMAND` |
| zsh | `precmd_functions` / `chpwd_functions` |
| fish | `fish_prompt` |
| PowerShell | 重定义 `prompt` 函数 |
| clink / nushell | 对应扩展钩子 |

- **生命周期**：`__PRISMENV_PID` / `__PRISMENV_SHELL` 环境变量 + `trap 'envhive env --cleanup' EXIT` 退出自动清理；
- **转义**：输出经 shell 专用转义（`BashEscape` / `PowerShellEscape`）防注入；
- **快速路径缓存**（vfox ConfigState 模式）：记录 `.envhive.toml` mtime + PATH 指纹，未变化跳过重算，避免拖慢提示符。

## G.工具源与插件机制（[P1·开放]）

### 内置工具定义（[P0·核心]）

```rust
pub struct ToolDef {
    pub name: String,        // "java" / "nodejs" / "go" / "rust"
    pub display: String,     // "Java (OpenJDK)"
    pub category: String,    // "language" | "tool" | "database"
    pub homepage: String,
    pub provider: VersionProvider,  // 枚举：Github / NodeDist / Adoptium / StaticIndex
}
```

### 远程注册表（[P1·开放]，对应 vfox manager_registry）

- 默认地址 `https://envhive.github.io/envhive-plugins`，`registry.address` 可自定义；
- Manifest（对应 vfox `RegistryPluginManifest`）：

```json
{ "name": "nodejs", "version": "1.0.0", "download_url": "...", "min_runtime_version": "0.1.0" }
```

- 流程：下载插件包 → 解压到 `~/.envhive/plugins/<name>/`。

### 插件机制选型（[P1·开放] / [P2]）

| 方案 | 档位 | 说明 | 借鉴对象 |
|---|---|---|---|
| **声明式 TOML 插件（首选）** | P1·开放 |工具源 = 一段 TOML：版本列表 URL + 包模板 + 平台映射，安全无需沙箱 | vfox 生态 metadata.lua 简化版 |
| Lua 插件 | P2（已实现 v2） | 完整脚本能力（http/json/archiver/file/versions 模块 + available/pre_install/post_install/env_keys/pre_uninstall hook，mlua vendored lua54；**v2 可信模型**：放开 os/io 全部标准库，网络白名单保留；发行商维度 + SDKMAN 风格标识符） | vfox `internal/plugin/luai` |

- **Hook 生命周期**（vfox 顺序）：`Available → PreInstall → (下载/校验/解压) → PostInstall → EnvKeys`；`Use` 前有 `preUse` 改写版本；
- Lua 支持用 `mlua` crate（对应 gopher-lua），编解码对应 vfox `luai/codec`；
- **安全边界（v2 可信模型）**：声明式插件不执行代码；Lua 插件放开全部标准库（可执行命令 / 读写文件），仅保留网络白名单（NETWORK_ALLOW）与 file 模块路径限制（~/.envhive 与临时目录）作为防误伤纵深 —— **只应从可信来源安装 Lua 插件**。

## H. 镜像源与仓库配置管理（[P1·体验]）

> 蜂巢独有模块（vfox 无对应），环境配置中枢的核心。

### 统一抽象：ToolRegistryWriter trait

```rust
pub trait ToolRegistryWriter: Send + Sync {
    fn tool_name(&self) -> &str;                        // "npm" / "pip" / "cargo" ...
    fn config_path(&self) -> PathBuf;                   // ~/.npmrc、settings.xml 等
    fn read_current(&self) -> Result<RegistryState>;    // 解析当前镜像配置
    fn apply(&self, preset: &RegistryPreset) -> Result<()>;  // 写入镜像 + 自动备份
    fn verify(&self) -> Result<()>;                     // 运行官方命令验证生效
    fn restore(&self) -> Result<()>;                    // 从备份还原
}

pub struct RegistryPreset {
    pub tool: String,            // "npm"
    pub name: String,            // "taobao"
    pub url: String,             // "https://registry.npmmirror.com"
    pub extra: HashMap<String, String>,  // trusted-host / sparse 标志 / scope 等
}
```

### 各工具配置文件与写入策略

|工具| 配置文件 | 格式 | 关键键 / 结构 |
|---|---|---|---|
| npm | `~/.npmrc` | key=value（ini 子集） | `registry=`、`proxy=`、`https-proxy=`、`strict-ssl=` |
| pip | `~/.pip/pip.conf` / `%APPDATA%\pip\pip.ini` | INI | `[global] index-url=`、`trusted-host=`、`proxy=` |
| cargo | `~/.cargo/config.toml` | TOML | `[source.crates-io] replace-with` + `[source.<mirror>] registry = "sparse+https://..."` |
| maven | `~/.m2/settings.xml` | XML | `<mirror><id>aliyun</id><mirrorOf>central</mirrorOf><url>...</url></mirror>` |
| go | `go env -w GOPROXY=...` | 键值（go 命令维护） | `GOPROXY` / `GONOSUMDB` / `GOPRIVATE` |

- **解析库**：npmrc 用 `ini`（或手写 ~50 行）；pip 用 `configparser`；cargo 复用 `toml`；settings.xml 用 `roxmltree` / `quick-xml`；
- **写入安全**：`读原文 → 备份 <file>.envhive.bak.<ts> → 精确 merge（只改目标键，保留注释）→ 原子写回`；
- **验证命令**：`npm config get registry` / `pip config list` / `cargo source` / `mvn help:effective-settings`（或解析回读）；
- **go 特殊处理**：不直接改文件，走 `go env -w` 交由 go工具链管理。

### 内置镜像预设

- npm：`registry.npmjs.org` ↔ npmmirror；pip：PyPI ↔ 清华 TUNA / 阿里云；cargo：crates.io ↔ rsproxy.cn（sparse）/ 中科大；maven：Central ↔ 阿里云 / 华为云；go：`proxy.golang.org` ↔ `goproxy.cn` / 七牛；[x] 已扩展 Docker（ustc/网易/百度）/ NuGet（tuna/华为/azurecn）/ RubyGems（阿里云/清华）/ pub.dev（sjtug/flutter-io）/ conda（tuna/ustc/阿里云）。

### proxy 两层联动（仅应用内生效）

> 设计决策：代理生效范围限定在蜂巢应用内（工具下载 / 版本列表 / 插件请求），**不写系统全局环境变量**
> （旧版第 3 层曾写用户注册表 HTTP_PROXY/HTTPS_PROXY/NO_PROXY，已移除，启动时清理历史残留）。

1. **蜂巢下载代理**：config.yaml `proxy.url / enable` → reqwest 全局 `Proxy`（工具下载 / 版本列表都走），Lua 插件 `http.get` 同步同一代理；
2. **工具代理**：开关打开时向各工具配置注入代理键（npm `~/.npmrc`、pip `pip.ini`）。

- **NO_PROXY**：旧版曾保留白名单配置（默认排除 `localhost, 127.0.0.1` 与内网网段），应用内生效策略下无实际用途，配置字段已删除；
- **可用性探测**：切换时对目标源 HEAD 请求，不通提示回退。

## I. 配置、依赖选型与事件总线（[P0·地基]）

### 全局配置 config.yaml（对应 vfox internal/config）

```yaml
proxy:
  url: "http://127.0.0.1:7890"
  enable: false
storage:
  tool_path: "~/.envhive/installs"     # 可自定义安装根目录
registry:
  address: "https://envhive.github.io/envhive-plugins"
cache:
  available_hook_duration: 12h      # 版本列表缓存 TTL；-1 永不过期，0 禁用
gitignore:
  enable: true                      # 项目 .envhive/tools 写入 .gitignore
```

- `serde_yaml` 反序列化，用户配置与内置默认 merge（对应 vfox `LoadConfigWithFallback`）；
- `storage` 路径启动时校验写权限，失败前端提示。

### vfox Go 依赖 → envhive Rust crate 对照

| vfox（Go） | 用途 | envhive（Rust） |
|---|---|---|
| urfave/cli/v3 | CLI 框架 | `clap`（CLI 子命令，shell hook 用） |
| gopher-lua | Lua 插件 VM | `mlua`（P2 已引入，vendored lua54 + serialize + send） |
| schollz/progressbar + pterm | 进度条 / 终端美化 | 前端渲染，后端仅 emit 事件 |
| go-winio + x/sys | Windows junction / 注册表 | `windows-sys`（或 `winreg` + `junction` crate） |
| bodgit/sevenzip + ulikunitz/xz + klauspost/compress | 解压 zip/7z/xz | `zip` + `tar` + `flate2` + `xz2` + `sevenz-rust` |
| gopsutil | 进程探测（tmp 清理） | `sysinfo` |
| BurntSushi/toml + yaml.v3 | 配置解析 | `toml` + `serde_yaml` |
| fuzzysearch | 版本/名称模糊搜索 | `fuzzy-matcher` |
| 自研 HTTP | 下载 | `reqwest`（内置代理 + Range） |
| 自研 cache | gob 文件缓存 | `serde_json` + TTL 文件缓存（自研 ~100 行） |

### 事件总线（Tauri 独有）

```rust
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub tool: String, pub version: String,
    pub percent: f32, pub speed_mbps: f64,
    pub stage: DownloadStage,   // resolving | downloading | verifying | extracting | done
}
```

所有长任务（下载 / 校验 / 安装 / 切换 / 镜像写入）通过 `AppHandle::emit` 推送，前端按 `stage` 渲染分阶段进度。

## J. 日志与错误处理（[P0·地基]）

- `tracing` + `tracing-appender`：按天滚动写入 `~/.envhive/logs/`，前端设置页"导出日志"；
- Tauri command 统一 `Result<T, EnvHiveError>`，`EnvHiveError` 实现 `Serialize`（结构化错误码，非仅 `Display`），前端可据 `code` 展示可操作提示（如"校验失败 → 重试 / 更换镜像"）；
- vfox `NotFoundError` 模式：`lookup_tool` 对未安装工具返回可恢复错误（`EnvHiveErrorKind::ToolNotFound`），上层决定是否触发 `lookup_sdk_with_install` 自动安装。

---

# 第三部分：版本里程碑与验收标准

## v0.1 MVP —— P0 · 地基 + P0 · 核心

**范围**：模块骨架（A/B/C/I/J）、内置 4工具、版本发现与缓存、下载/校验/解压/安装/卸载、全局版本切换（三 scope 配置链 + symlink/junction）、当前版本查询。

**验收标准**：
- Windows：`安装 Node 22.11.0 → 注册表 PATH 生效（新终端 node -v 正确）→ 切换到 21.5.1 即时生效 → 卸载干净`；
- macOS / Linux：同一流程（symlink + profile 注入）；
- 全程前端可见分阶段进度与错误提示；断网 / 校验失败 / 解压失败均有明确错误码与清理动作。

## v1.0 —— P1 · 体验 + P1 · 开放

**范围**：镜像源与仓库配置、proxy 管理、下载队列、冲突检测、系统托盘、进程注入 Session、TOML 插件 + 远程注册表、Shell Hook。

**验收标准**：
- 一键切换 `npm → npmmirror` 后 `npm config get registry` 立即返回新源，且原 `.npmrc` 其他配置与注释保留；
- 切换代理开关后，蜂巢工具下载、版本列表与插件请求走该代理，且不写入系统全局环境变量（`HTTP_PROXY` 等不受影响）；
- 终端内 `cd` 进入带 `.envhive.toml` 的项目自动切换版本（Shell Hook）；
- 从注册表安装任意 TOML 插件定义的新工具并完成安装切换。

## v1.1 —— P2 · 增殖

**范围**：Lua 插件、镜像预设扩展、系统代理联动、开机自启动、下载加速镜像、使用统计。

**验收标准**：Lua 插件通过 `available → pre_install → env_keys` 完整生命周期安装一个自定义工具。

## v1.2+ —— P3 · 远期

**范围**：自动更新、插件市场托管与审核、环境智能诊断。

---

# 第四部分：关键依赖关系

```
P0 · 核心 ──依赖──> P0 · 地基（模块骨架 / 配置 / 事件 / 日志 / 配置链）
  │
  ├─ 版本发现 ──>工具注册表（源定义）
  ├─ 下载安装 ──> 版本发现（拿版本）＋ 平台检测
  ├─ 全局切换 ──> 配置链（写 scope）＋ 存储模型（symlink）
  └─ 卸载 ──────> 存储模型

P1 · 体验
  ├─ 进程注入 Session ──> Envs 合并（spawn_with_env）
  └─ 下载队列 / 通知 ──> 事件总线

P1 · 开放
  ├─ Shell Hook ──────> 配置链 ＋ CLI 子命令（clap）
  ├─ TOML 插件 ──────> 下载管道（插件包下载）＋ registry manifest
  └─ 应用内 Session ──> Envs 合并

P2 / P3 ──依赖──> P1 全部（生态能力建立在开放机制之上）
```
