// AppStore：Vue 3 全局状态层（由 React store.tsx 迁移）
// 模块级 reactive 单例 —— 组件通过 useApp() 读取，天然响应式，无需 Provider。
// 全局提示 / 确认框使用 Naive UI discrete API（组件外可用）。
import { reactive } from "vue";
import { createDiscreteApi } from "naive-ui";
import type {
  BootstrapInfo,
  ConflictInfo,
  DownloadMirrorConfig,
  DownloadProgress,
  HomeOverview,
  LogFileInfo,
  PageKey,
  PluginInfo,
  ProjectPreset,
  ProxyConfig,
  QueueEnqueueResult,
  QueueTask,
  RegistryEntry,
  RemotePluginInfo,
  RegistryState,
  ToolInfo,
  ToolSelection,
  SettingsConfig,
  SwitchResult,
  UsageStats,
  PresetInfo,
} from "./types";
import { REGISTRY_TOOLS, lastNonFx, versionsKey } from "./types";
import { notify, run, setupEvents } from "./hooks/useBackend";

// ---------- 全局 UI 能力（Naive UI discrete API） ----------
const { message, dialog } = createDiscreteApi(["message", "dialog"]);

export function showMsg(m: string) {
  message.info(m);
}
export function showErr(m: string) {
  message.error(m);
}

/**
 * 统一错误信息提取：Tauri invoke 的 reject 值是 `{ code, message }` 对象，
 * 直接 String(e) 会得到 "[object Object]"，无法展示真实原因。
 * 兼容字符串 / Error / 结构化对象 / JSON 字符串等各类形态。
 */
export function errMsg(e: unknown): string {
  if (e == null) return "未知错误";
  if (typeof e === "string") {
    // Tauri 可能把 {code,message} 序列化成 JSON 字符串
    const t = e.trim();
    if (t.startsWith("{")) {
      try {
        const obj = JSON.parse(t) as { message?: unknown };
        if (typeof obj.message === "string" && obj.message) return obj.message;
      } catch {
        /* 不是 JSON，按原样返回 */
      }
    }
    return t;
  }
  if (e instanceof Error) return e.message || String(e);
  if (typeof e === "object") {
    const obj = e as { message?: unknown; code?: unknown; toString?: () => string };
    if (typeof obj.message === "string" && obj.message) return obj.message;
    // 自定义 toString（如 [Io] 拒绝访问 这类描述）
    try {
      const s = obj.toString?.();
      if (typeof s === "string" && s && s !== "[object Object]") return s;
    } catch {
      /* ignore */
    }
    try {
      return JSON.stringify(e);
    } catch {
      return String(e);
    }
  }
  return String(e);
}
/** 确认框：返回用户是否确认 */
export function confirmAsk(title: string, content: string): Promise<boolean> {
  return new Promise((resolve) => {
    dialog.warning({
      title,
      content,
      positiveText: "确定",
      negativeText: "取消",
      onPositiveClick: () => resolve(true),
      onNegativeClick: () => resolve(false),
      onClose: () => resolve(false),
      onMaskClick: () => resolve(false),
    });
  });
}

// ---------- 状态 ----------
// 预览模式工具样例（仅未连接 Rust 后端时注入，便于浏览器预览 UI；连上后端由真实数据覆盖）
const PREVIEW_TOOLS: ToolInfo[] = [
  {
    name: "go",
    display: "Go",
    category: "language",
    homepage: "https://go.dev",
    current: "1.25.12",
    installed: ["1.25.12", "1.24.3"],
    available: null,
    binPath: "C:\\go\\bin\\go.exe",
    distributions: [{ key: "official", display: "官方版" }],
  },
  {
    name: "java",
    display: "Java",
    category: "language",
    homepage: "https://adoptium.net",
    current: "21.0.5",
    installed: ["21.0.5", "17.0.12", "11.0.23"],
    available: null,
    binPath: "C:\\Program Files\\Eclipse Adoptium\\jdk-21.0.5\\bin\\java.exe",
    distributions: [
      { key: "temurin", display: "Eclipse Temurin" },
      { key: "zulu", display: "Azul Zulu" },
    ],
    defaultDistribution: "temurin",
  },
  {
    name: "node",
    display: "Node.js",
    category: "runtime",
    homepage: "https://nodejs.org",
    current: "22.14.0",
    installed: ["22.14.0", "20.19.1", "18.20.4-fx-1"],
    available: null,
    binPath: null,
    distributions: [{ key: "official", display: "官方版" }],
  },
  {
    name: "python",
    display: "Python",
    category: "language",
    homepage: "https://www.python.org",
    current: "3.13.1",
    installed: ["3.13.1", "3.12.8", "3.11.11"],
    available: null,
    binPath: "C:\\Python313\\python.exe",
    mirrors: [
      {
        name: "ghproxy",
        from: "https://github.com/astral-sh/python-build-standalone/",
        to: "https://ghproxy.net/https://github.com/astral-sh/python-build-standalone/",
      },
    ],
  },
  {
    name: "rust",
    display: "Rust",
    category: "language",
    homepage: "https://rust-lang.org",
    current: null,
    installed: [],
    available: null,
    binPath: null,
    distributions: [{ key: "official", display: "官方版" }],
  },
];

export const LUA_SAMPLE = `TOOL = {
  name = "hello",
  display = "Hello (示例)",
  category = "language",
  homepage = "https://example.com",
  verify_bin = "echo",
  verify_arg = "hello",
  bin_suffix = "",
}

function available()
  return { "1.0.0", "1.1.0" }
end

function pre_install(ctx)
  return { url = "https://example.com/hello-" .. ctx.version .. ".zip" }
end

function env_keys(ctx)
  return { vars = { HELLO_HOME = ctx.root } }
end`;

const state = reactive({
  // 壳
  page: "home" as PageKey,
  queueDrawerOpen: false,
  backend: false,

  //工具
  tools: [] as ToolInfo[],
  selections: {} as Record<string, ToolSelection>,
  versionsMap: {} as Record<string, string[]>,
  refreshing: {} as Record<string, boolean>,
  bootstrap: null as BootstrapInfo | null,
  home: null as HomeOverview | null,

  // 队列
  queue: [] as QueueTask[],

  // 网络 / 代理
  registry: {} as Record<string, RegistryState | null>,
  presets: {} as Record<string, PresetInfo[]>,
  proxy: null as ProxyConfig | null,
  proxyUrl: "",
  proxyEnable: false,
  mirrorCfg: null as DownloadMirrorConfig | null,
  autostart: false,
  trayResident: false,
  conflicts: [] as ConflictInfo[],

  // 插件 / 统计
  remotePlugins: [] as RemotePluginInfo[],
  remotePluginsLoading: false,
  remotePluginsError: "",
  registryEntries: [] as RegistryEntry[],
  remoteRegistry: "",
  plugins: [] as PluginInfo[],
  usageStats: null as UsageStats | null,
  luaName: "",
  luaScript: "",

  // 设置
  cacheTtl: "",
  storagePath: "",
  exportText: null as string | null,
  importText: "",

  // 日志
  logsFiles: [] as LogFileInfo[],
  logsContent: "",
  logsDir: "",
  logLevel: "ALL",
  logFile: "",

  // 项目预设
  projects: [] as ProjectPreset[],

  // busy 状态（正在执行的工具名）
  busy: null as string | null,
  progress: null as DownloadProgress | null,

  // 首次 loadAll() 完成标记：true 之前内容区展示统一启动占位，避免先渲染
  // 骨架屏 / "预览模式"警告再被数据覆盖造成的视觉闪烁（FOUC）
  appLoaded: false,
});

/** 后端返回的旧版纯字符串仓库地址 → 自动生成仓库名（与后端 auto_registry_name 规则一致） */
function registryName(url: string, index: number): string {
  const lower = url.toLowerCase();
  if (lower.includes("gitee")) return "官方gitee";
  if (lower.includes("github")) return "官方github";
  const host = url.split("://")[1]?.split(/[/?#]/)[0];
  return host ? `${host} 仓库` : `仓库${index + 1}`;
}

/** 归一化后端 registry.addresses（兼容 {name,url} 对象与旧版纯字符串） */
export function normalizeRegistryEntries(raw: (RegistryEntry | string)[]): RegistryEntry[] {
  return raw
    .map((a, i) => {
      if (typeof a === "string") {
        const url = a.trim().replace(/\/+$/, "");
        return url ? { name: registryName(url, i), url } : null;
      }
      const url = (a.url ?? "").trim().replace(/\/+$/, "");
      const name = (a.name ?? "").trim();
      if (!url) return null;
      return { name: name || registryName(url, i), url };
    })
    .filter((e): e is RegistryEntry => e !== null);
}

// ---------- DEV-only mock ----------
// `?mockplugins=1` query 触发，注入 6 个市场插件示例数据，便于浏览器裸前端预览布局；
// `import.meta.env.DEV` 守卫让生产构建由 Vite 整段 tree-shake 消除，不影响产物。
if (import.meta.env.DEV && typeof window !== "undefined" && new URLSearchParams(window.location.search).get("mockplugins") === "1") {
  const now = Math.floor(Date.now() / 1000);
  state.registryEntries = [
    { name: "官方gitee", url: "https://raw.giteeusercontent.com/envhive/envhive/raw/main/manifest.json" },
    { name: "官方github", url: "https://raw.githubusercontent.com/envhive/envhive/main/manifest.json" },
  ];
  state.remoteRegistry = state.registryEntries[0].url;
  state.remotePlugins = [];
  state.remotePluginsLoading = false;
  state.remotePluginsError = "";
  state.plugins = [
    { name: "nodejs", display: "Node.js", category: "runtime", homepage: "https://nodejs.org", provider: "lua", version: "1.0.0", installedVersions: ["22.14.0", "20.19.1", "18.20.4"], source: "market", distributions: [], path: "/Users/admin/.envhive/plugins/nodejs", updatedAt: now - 18 * 60, enabled: true },
    { name: "java", display: "Java", category: "language", homepage: "https://adoptium.net", provider: "lua", version: "1.0.0", installedVersions: ["21.0.5-zulu", "17.0.12-tem", "11.0.23-tem"], source: "market", distributions: [{ key: "open", display: "OpenJDK (Eclipse)" }, { key: "tem", display: "Eclipse Temurin" }, { key: "zulu", display: "Azul Zulu" }, { key: "corretto", display: "Corretto (Amazon)" }, { key: "graal", display: "GraalVM (Oracle)" }, { key: "bisheng", display: "Bisheng (Huawei)" }, { key: "librca", display: "Liberica (BellSoft)" }, { key: "albba", display: "Dragonwell (Alibaba)" }], path: "/Users/admin/.envhive/plugins/java", updatedAt: now - 18 * 60, enabled: true },
    { name: "go", display: "Go", category: "language", homepage: "https://go.dev", provider: "lua", version: "1.0.0", installedVersions: ["1.25.12"], source: "market", distributions: [], path: "/Users/admin/.envhive/plugins/go", updatedAt: now - 18 * 60, enabled: true },
    { name: "rust", display: "Rust", category: "language", homepage: "https://rust-lang.org", provider: "lua", version: "1.0.0", installedVersions: [], source: "market", distributions: [], path: "/Users/admin/.envhive/plugins/rust", updatedAt: now - 18 * 60, enabled: true },
    { name: "python", display: "Python", category: "language", homepage: "https://www.python.org", provider: "lua", version: "1.0.0", installedVersions: ["3.13.1"], source: "market", distributions: [], path: "/Users/admin/.envhive/plugins/python", updatedAt: now - 18 * 60, enabled: true },
    { name: "maven", display: "Maven", category: "build", homepage: "https://maven.apache.org", provider: "lua", version: "1.0.0", installedVersions: ["3.9.16", "3.9.9"], source: "market", distributions: [], path: "/Users/admin/.envhive/plugins/maven", updatedAt: now - 18 * 60, enabled: true },
  ];
}

// ---------- 数据加载 ----------
async function loadTools() {
  try {
    const list = await run<ToolInfo[]>("list_tools");
    state.tools = list;
    // list_tools 成功即证明后端已连接：禁用全部工具时后端返回空列表（并非预览模式），
    // 置位 backend 避免页面把「空工具列表」误判为「未连接 Rust 后端」而展示预览模式警告
    state.backend = true;
    for (const s of list) {
      if (!state.selections[s.name]) {
        const dist =
          s.distributions && s.distributions.length > 0
            ? (s.defaultDistribution ?? s.distributions[0].key)
            : undefined;
        state.selections[s.name] = { dist, version: "" };
      }
    }
  } catch (e) {
    // 未连接后端 → 预览模式：注入样例数据便于浏览器预览 UI
    state.tools = PREVIEW_TOOLS;
    showMsg(`预览模式：${errMsg(e)}`);
  }
}

async function loadHome() {
  try {
    state.home = await run<HomeOverview>("home_overview");
  } catch {
    // 预览模式忽略
  }
}

async function loadQueue() {
  try {
    state.queue = await run<QueueTask[]>("queue_status");
  } catch {
    // 预览模式忽略
  }
}

async function loadNetwork() {
  try {
    const p = await run<ProxyConfig>("get_proxy");
    state.proxy = p;
    state.proxyUrl = p?.url ?? "";
    state.proxyEnable = p?.enable ?? false;
    for (const t of REGISTRY_TOOLS) {
      run<RegistryState>("get_registry_state", { tool: t })
        .then((s) => (state.registry[t] = s))
        .catch(() => {});
      run<PresetInfo[]>("list_registry_presets", { tool: t })
        .then((s) => (state.presets[t] = s))
        .catch(() => {});
    }
  } catch {
    // 预览模式忽略
  }
}

async function loadExtras() {
  run<ConflictInfo[]>("check_conflicts").then((v) => (state.conflicts = v)).catch(() => {});
}

/** 拉取远程插件列表（address 缺省时用当前选中的仓库，其次后端第一个已配置地址） */
async function loadRemotePlugins(address?: string) {
  state.remotePluginsLoading = true;
  state.remotePluginsError = "";
  const target = address || state.remoteRegistry || undefined;
  try {
    const list = await run<RemotePluginInfo[]>(
      "list_remote_plugins",
      target ? { address: target } : undefined
    );
    state.remotePlugins = list;
    if (target) state.remoteRegistry = target;
  } catch (e) {
    state.remotePlugins = [];
    state.remotePluginsError = errMsg(e);
  } finally {
    state.remotePluginsLoading = false;
  }
}

/** 插件市场切换注册表地址：记住选择（下次启动默认展示）并立即刷新插件列表 */
async function switchRemoteRegistry(addr: string) {
  if (state.remoteRegistry === addr) return;
  state.remoteRegistry = addr;
  // 持久化选中仓库（config.yaml registry.selected），下次启动恢复
  run<void>("set_registry_selected", { url: addr }).catch(() => {});
  await loadRemotePlugins(addr);
}

async function loadP2() {
  run<boolean>("get_autostart").then((v) => (state.autostart = v)).catch(() => {});
  run<boolean>("get_tray_resident").then((v) => (state.trayResident = v)).catch(() => {});
  run<DownloadMirrorConfig>("get_download_mirror").then((v) => (state.mirrorCfg = v)).catch(() => {});
  run<UsageStats>("get_usage_stats").then((v) => (state.usageStats = v)).catch(() => {});
  run<PluginInfo[]>("list_plugins").then((v) => (state.plugins = v)).catch(() => {});
}

async function loadSettings() {
  try {
    const cfg = await run<SettingsConfig>("get_config");
    state.cacheTtl = cfg.cache?.availableHookDuration ?? "";
    // 多地址优先；兼容旧版单 address 字段
    const raw = cfg.registry?.addresses?.length
      ? cfg.registry.addresses
      : cfg.registry?.address
        ? [cfg.registry.address]
        : [];
    state.registryEntries = normalizeRegistryEntries(raw);
    const urls = state.registryEntries.map((e) => e.url);
    // 优先恢复上次切换选中的仓库（config.yaml registry.selected）；不在列表中则回退第一个
    const saved = cfg.registry?.selected;
    state.remoteRegistry = saved && urls.includes(saved) ? saved : (state.registryEntries[0]?.url ?? "");
    state.storagePath = cfg.storage?.toolPath ?? "";
  } catch {
    // 预览模式忽略
  }
}

async function loadProjects() {
  run<ProjectPreset[]>("list_projects")
    .then((v) => (state.projects = v))
    .catch((e) => console.warn("[envhive] 加载项目预设失败:", errMsg(e)));
}

async function refreshLogs() {
  try {
    const files = await run<LogFileInfo[]>("list_logs");
    state.logsFiles = files;
    state.logsDir = state.logsDir || state.bootstrap?.logsDir || "";
    const name = state.logFile || files[0]?.name || "";
    const content = await run<string>("read_logs", { file: name || null, level: state.logLevel });
    state.logsContent = content;
    if (name) state.logFile = name;
  } catch (e) {
    showErr(`读取日志失败：${errMsg(e)}`);
  }
}

async function loadAll() {
  await Promise.allSettled([
    loadTools(),
    loadHome(),
    loadQueue(),
    loadNetwork(),
    loadExtras(),
    loadP2(),
    loadSettings(),
    loadProjects(),
  ]);
  // 插件市场列表：在 loadSettings 恢复选中仓库之后再拉取，确保显示上次切换的仓库
  await loadRemotePlugins();
  const b = await run<BootstrapInfo>("bootstrap").catch(() => null);
  state.bootstrap = b;
  if (b?.logsDir) state.logsDir = b.logsDir;
  await refreshLogs().catch(() => {
    // 日志读取失败不影响应用启动：避免阻塞 appLoaded 翻为 true
  });
  // 无论命令成功 / 失败 / 超时，首次拉取结束即可解锁内容区展示，
  // 避免停留在骨架屏 + "预览模式"的中间态导致视觉闪烁
  state.appLoaded = true;
}

// ---------- 事件订阅（注册一次） ----------
let eventsCleanup: (() => void) | null = null;

export function initEvents(): () => void {
  if (eventsCleanup) return eventsCleanup;
  let cleanup: (() => void) | undefined;
  void setupEvents({
    onProgress: (p) => {
      state.progress = p;
      // 合并实时进度到队列任务：后端 queue-updated 的 percent 只在 0%/100% 两个端点更新，
      // 真实进度通过 download-progress 推送。worker 串行，同一时刻至多一个 running 任务，
      // 按 tool+version 匹配即可安全回填。
      const q = state.queue.find(
        (t) => t.status === "running" && t.tool === p.tool && t.version === p.version
      );
      if (q) {
        q.percent = p.percent;
        q.stage = p.stage;
        q.speedMbps = p.speedMbps;
        q.url = p.url;
        q.totalBytes = p.totalBytes;
        q.downloadedBytes = p.downloadedBytes;
        // 附加提示（如镜像失败回退官方源）：写入队列消息 + 界面通知
        if (p.note) {
          q.message = p.note;
          showMsg(`${p.tool} ${p.version}：${p.note}`);
        }
      }
      if (p.stage === "done") {
        state.busy = null;
        const key = `${p.tool}|${p.version}`;
        window.setTimeout(() => {
          const cur = state.progress;
          if (cur && `${cur.tool}|${cur.version}` === key) state.progress = null;
        }, 1500);
        void notify("蜂巢 EnvHive · 安装完成", `${p.tool} ${p.version} 安装完成`);
      }
      if (p.stage === "failed") {
        state.busy = null;
        state.progress = null;
      }
    },
    onQueue: (q) => (state.queue = q),
    onDone: () => {
      void loadAll();
    },
    onError: (message) => {
      state.busy = null;
      state.progress = null;
      showErr(`安装失败：${message}`);
      void loadAll();
    },
  })().then((fn) => (cleanup = fn));
  eventsCleanup = () => cleanup?.();
  return eventsCleanup;
}

// ---------- 操作 ----------
async function loadVersions(tool: ToolInfo, force = false) {
  const sel = state.selections[tool.name];
  const dist = sel?.dist;
  const key = versionsKey(tool.name, dist);
  state.refreshing[key] = true;
  try {
    const versions = await run<string[]>("get_versions", {
      name: tool.name,
      distribution: dist ?? null,
      refresh: force,
    });
    state.versionsMap[key] = versions;
    const cur = state.selections[tool.name];
    const version = cur?.version || lastNonFx(versions);
    state.selections[tool.name] = { dist, version };
    showMsg(`${tool.display}${dist ? ` · ${dist}` : ""} 版本列表已刷新（${versions.length} 个版本）`);
  } catch (e) {
    showErr(`获取 ${tool.display} 版本失败：${errMsg(e)}`);
  } finally {
    state.refreshing[key] = false;
  }
}

async function install(tool: ToolInfo) {
  const sel = state.selections[tool.name];
  const version = sel?.version;
  if (!version) return showMsg(`请先为 ${tool.display} 选择版本`);
  state.busy = tool.name;
  try {
    const r = await run<QueueEnqueueResult>("enqueue_install", {
      name: tool.name,
      version,
      distribution: sel.dist ?? null,
    });
    if (r.reused) {
      showMsg(`${tool.display} ${version} 已在队列中（任务 #${r.id}），无需重复入队`);
    } else {
      showMsg(`${tool.display} ${version} 已加入队列（任务 #${r.id}）`);
    }
    await loadQueue();
  } catch (e) {
    showErr(`加入队列失败：${errMsg(e)}`);
  } finally {
    state.busy = null;
  }
}

async function switchVersion(tool: ToolInfo, version: string) {
  state.busy = tool.name;
  try {
    const r = await run<SwitchResult>("switch_version", { name: tool.name, version });
    showMsg(`${tool.display} → ${r.version}：${r.message}（新终端生效，已打开窗口需重启）`);
    await Promise.all([loadTools(), loadQueue(), loadHome()]);
  } catch (e) {
    showErr(`切换失败：${errMsg(e)}`);
  } finally {
    state.busy = null;
  }
}

/** 取消某工具的全局设置：解除激活并从 PATH / *_HOME 等环境变量移除（不卸载版本） */
async function unuseGlobal(tool: ToolInfo) {
  if (!tool.current) {
    showMsg(`${tool.display} 当前未设置为全局版本`);
    return;
  }
  const envName = tool.name.toUpperCase() + "_HOME";
  const ok = await confirmAsk(
    `取消 ${tool.display} 的全局设置`,
    `将解除 ${tool.display} ${tool.current} 的全局激活，并从 PATH / ${envName} 等环境变量中移除（不会卸载该版本）。新终端生效，已打开窗口需重启。`
  );
  if (!ok) return;
  state.busy = tool.name;
  try {
    await run<void>("unuse_global", { name: tool.name });
    showMsg(`${tool.display} 已取消全局设置，恢复系统默认`);
    await Promise.all([loadTools(), loadQueue(), loadHome()]);
  } catch (e) {
    showErr(`取消失败：${errMsg(e)}`);
  } finally {
    state.busy = null;
  }
}

async function uninstallVersion(tool: ToolInfo, version: string) {
  const ok = await confirmAsk(
    `卸载 ${tool.display} ${version}`,
    "已安装版本目录将被删除，可通过重新安装恢复。"
  );
  if (!ok) return;
  state.busy = tool.name;
  try {
    await run<void>("uninstall_tool", { name: tool.name, version });
    showMsg(`${tool.display} ${version} 已卸载`);
    await Promise.all([loadTools(), loadHome(), loadP2()]);
  } catch (e) {
    showErr(`卸载失败：${errMsg(e)}`);
  } finally {
    state.busy = null;
  }
}

async function uninstall(tool: ToolInfo) {
  const version = state.selections[tool.name]?.version || tool.current;
  if (!version) return showMsg(`没有可卸载的版本`);
  const ok = await confirmAsk(
    `卸载 ${tool.display} ${version}`,
    "下拉框当前选中版本。已安装版本目录将被删除，可通过重新安装恢复。"
  );
  if (!ok) return;
  state.busy = tool.name;
  try {
    await run<void>("uninstall_tool", { name: tool.name, version });
    showMsg(`${tool.display} ${version} 已卸载`);
    await Promise.all([loadTools(), loadHome(), loadP2()]);
  } catch (e) {
    showErr(`卸载失败：${errMsg(e)}`);
  } finally {
    state.busy = null;
  }
}

// ---- 网络操作 ----
async function applyPreset(tool: string, name: string) {
  try {
    await run<void>("apply_registry", { tool, preset: name });
    showMsg(`${tool} 镜像已切换为 ${name}`);
    state.registry[tool] = await run<RegistryState>("get_registry_state", { tool });
  } catch (e) {
    showErr(`镜像切换失败：${errMsg(e)}`);
  }
}

async function reloadToolPresets(tool: string) {
  const [s, ps] = await Promise.all([
    run<RegistryState>("get_registry_state", { tool }).catch(() => null),
    run<PresetInfo[]>("list_registry_presets", { tool }).catch(() => []),
  ]);
  if (s) state.registry[tool] = s;
  state.presets[tool] = ps;
}

async function addCustomPreset(tool: string, name: string, url: string) {
  try {
    await run<void>("add_custom_registry_preset", { tool, name, url });
    showMsg(`${tool} 已添加自定义源「${name}」`);
    await reloadToolPresets(tool);
  } catch (e) {
    showErr(`添加自定义源失败：${errMsg(e)}`);
  }
}

async function removeCustomPreset(tool: string, name: string) {
  const ok = await confirmAsk(
    `删除自定义源「${name}」`,
    `删除 ${tool} 自定义源「${name}」？（不影响已写入的配置文件）`
  );
  if (!ok) return;
  try {
    await run<void>("remove_custom_registry_preset", { tool, name });
    showMsg(`${tool} 自定义源「${name}」已删除`);
    await reloadToolPresets(tool);
  } catch (e) {
    showErr(`删除自定义源失败：${errMsg(e)}`);
  }
}

async function saveProxy(silent = false) {
  try {
    await run<void>("set_proxy", { url: state.proxyUrl || null, enable: state.proxyEnable });
    if (!silent) showMsg(state.proxyEnable ? `代理已启用：${state.proxyUrl}` : "代理已关闭");
    state.proxy = await run<ProxyConfig>("get_proxy");
  } catch (e) {
    showErr(`代理设置失败：${errMsg(e)}`);
  }
}

async function cancelTask(id: number) {
  try {
    await run<void>("cancel_task", { id });
    showMsg(`任务 #${id} 已取消`);
    await loadQueue();
  } catch (e) {
    showErr(`取消失败：${errMsg(e)}`);
  }
}

/** 全部取消：排队中直接取消，执行中请求取消（worker 收尾定终态） */
async function cancelAll() {
  const ok = await confirmAsk("全部取消", "将取消队列中所有任务，正在执行的任务会在下载/校验节点尽快终止。");
  if (!ok) return;
  try {
    const n = await run<number>("queue_cancel_all");
    showMsg(n > 0 ? `已请求取消 ${n} 个任务` : "当前没有可取消的任务");
    await loadQueue();
  } catch (e) {
    showErr(`全部取消失败：${errMsg(e)}`);
  }
}

/** 清空已完成/失败/已取消的任务，保留排队中与执行中 */
async function clearFinished() {
  try {
    const n = await run<number>("queue_clear_finished");
    showMsg(n > 0 ? `已清空 ${n} 个已完成任务` : "没有可清理的已完成任务");
    await loadQueue();
  } catch (e) {
    showErr(`清空失败：${errMsg(e)}`);
  }
}

// ---- 冲突 ----
async function ackConflict(c: ConflictInfo) {
  if (c.kind !== "registry") return;
  const ok = await confirmAsk(
    "确认当前配置为基线？",
    `${c.message}\n\n确认后冲突提示将消除（后续再次被外部修改会重新提示）。`
  );
  if (!ok) return;
  try {
    await run<void>("ack_registry_conflict", { tool: c.tool });
    showMsg(`${c.tool} 指纹已重新记录`);
    loadExtras();
  } catch (e) {
    showErr(`操作失败：${errMsg(e)}`);
  }
}

async function fixToolConflict(c: ConflictInfo) {
  if (c.kind !== "tool" || !c.version) return;
  try {
    await run<SwitchResult>("switch_version", { name: c.tool, version: c.version });
    showMsg(`${c.tool} 已重新切换至 ${c.version}`);
    await Promise.all([loadExtras(), loadTools(), loadHome()]);
  } catch (e) {
    showErr(`修复失败：${errMsg(e)}`);
  }
}

async function installRemotePlugin(p: RemotePluginInfo) {
  // 同名插件已装且版本不同 → 视为「更新」（本地未声明版本的插件视为自定义，不提示更新）
  const local = state.plugins.find((x) => x.name === p.name);
  const isUpdate = !!local?.version && local.version !== p.version;
  const ok = await confirmAsk(
    isUpdate ? `更新插件 ${p.name}：v${local!.version} → v${p.version}` : `安装远程插件 ${p.name}@${p.version}`,
    p.description || "（无描述）"
  );
  if (!ok) return;
  try {
    // 整个插件对象传入：type / format / sha256 随包校验（后端 zip 安装链路）
    await run<void>("install_remote_plugin", { plugin: p });
    showMsg(`插件 ${p.name} 已${isUpdate ? "更新" : "安装"}至 v${p.version}`);
    await Promise.all([loadTools(), loadExtras(), loadP2(), loadRemotePlugins()]);
  } catch (e) {
    showErr(`${isUpdate ? "更新" : "安装"}插件失败：${errMsg(e)}`);
  }
}

// ---- P2 操作 ----
async function toggleAutostart() {
  state.busy = "autostart";
  try {
    const next = !state.autostart;
    await run<void>("set_autostart", { enable: next });
    state.autostart = next;
    showMsg(next ? "已开启开机自启动（开机自动启动，窗口隐藏驻留托盘）" : "已关闭开机自启动");
  } catch (e) {
    showErr(`设置自启动失败：${errMsg(e)}`);
  } finally {
    state.busy = null;
  }
}

async function toggleTrayResident() {
  state.busy = "trayResident";
  try {
    const next = !state.trayResident;
    await run<void>("set_tray_resident", { enable: next });
    state.trayResident = next;
    showMsg(next ? "已开启托盘常驻：关闭窗口后将隐藏到系统托盘，后台继续运行" : "已关闭托盘常驻");
  } catch (e) {
    showErr(`设置托盘常驻失败：${errMsg(e)}`);
  } finally {
    state.busy = null;
  }
}

/** 某工具当前生效的加速镜像名：优先命中用户/插件规则（rules[from]==to 且非官方），否则 null = 官方源 */
export function currentToolMirror(tool: ToolInfo): string | null {
  if (!tool.mirrors || tool.mirrors.length === 0) return null;
  const rules = state.mirrorCfg?.rules ?? {};
  // 官方候选（from == to）不算镜像；匹配 to 值的候选视为当前选中
  for (const m of tool.mirrors) {
    if (m.from === m.to) continue;
    if (rules[m.from] === m.to) return m.name;
  }
  return null;
}

/** 切换某工具的下载加速镜像（mirror = 插件声明的镜像名；null = 官方源） */
async function setToolMirror(tool: ToolInfo, mirror: string | null) {
  try {
    const cfg = await run<DownloadMirrorConfig>("set_tool_mirror", {
      tool: tool.name,
      mirror,
    });
    state.mirrorCfg = cfg;
    showMsg(mirror ? `${tool.display} 下载加速镜像已切换为「${mirror}」` : `${tool.display} 已恢复官方源`);
  } catch (e) {
    showErr(`切换镜像失败：${errMsg(e)}`);
  }
}

async function addLuaPlugin() {
  if (!state.luaName.trim() || !state.luaScript.trim()) return showMsg("请填写插件名称与脚本");
  try {
    await run<void>("add_lua_plugin", { name: state.luaName.trim(), script: state.luaScript });
    showMsg(`Lua 插件 ${state.luaName.trim()} 已添加`);
    state.luaName = "";
    state.luaScript = "";
    await Promise.all([loadTools(), loadP2()]);
  } catch (e) {
    showErr(`添加 Lua 插件失败：${errMsg(e)}`);
  }
}

/** 更新已有插件源码（编辑模式；Lua 校验 + 原子写回） */
async function saveLuaPlugin(name: string, provider: string, script: string) {
  if (!script.trim()) return showMsg("脚本为空");
  try {
    await run<void>("save_plugin_source", { name, provider, script });
    showMsg(`插件 ${name} 已保存`);
    await Promise.all([loadTools(), loadP2()]);
  } catch (e) {
    showErr(`保存插件失败：${errMsg(e)}`);
  }
}

/** 读取插件源码（编辑器加载） */
async function loadPluginSource(name: string): Promise<{ name: string; provider: string; script: string } | null> {
  try {
    return await run<{ name: string; provider: string; script: string }>("read_plugin_source", { name });
  } catch (e) {
    showErr(`读取插件失败：${errMsg(e)}`);
    return null;
  }
}

/** 禁用 / 启用插件 */
async function togglePlugin(name: string, enable: boolean) {
  try {
    await run<void>("toggle_plugin", { name, enable });
    showMsg(`插件 ${name} 已${enable ? "启用" : "禁用"}`);
    await Promise.all([loadTools(), loadP2()]);
  } catch (e) {
    showErr(`${enable ? "启用" : "禁用"}插件失败：${errMsg(e)}`);
  }
}

/** 删除插件（确认由调用方弹出；仅删插件定义，已安装版本保留） */
async function deletePlugin(name: string, display: string) {
  const ok = await confirmAsk(
    `删除插件 ${name}`,
    `将删除插件「${display}」（${name}）的定义目录。\n\n已安装的版本文件会保留在缓存中，可在「统计」页清理。`
  );
  if (!ok) return;
  try {
    await run<void>("delete_plugin", { name });
    showMsg(`插件 ${name} 已删除`);
    await Promise.all([loadTools(), loadP2()]);
  } catch (e) {
    showErr(`删除插件失败：${errMsg(e)}`);
  }
}

/** 在系统文件管理器中打开插件目录 */
async function openPluginDir(name: string) {
  try {
    await run<void>("open_plugin_dir", { name });
  } catch (e) {
    showErr(`打开插件目录失败：${errMsg(e)}`);
  }
}

async function saveSettings(silent = false) {
  const entries = state.registryEntries
    .map((e) => ({ name: (e.name ?? "").trim(), url: (e.url ?? "").trim() }))
    .filter((e) => e.url);
  if (entries.length === 0) return showErr("至少需要配置一个插件仓库地址");
  const oldRegistry = state.remoteRegistry;
  try {
    await run<void>("update_config", {
      cacheTtl: state.cacheTtl.trim() || null,
      registryEntries: entries,
      storagePath: state.storagePath.trim() || null,
    });
    if (!silent) showMsg("配置已保存（存储路径修改需重启应用生效）");
    await loadSettings();
    // 当前选中地址被删除时，回退到新列表并刷新插件市场
    if (!entries.some((e) => e.url === oldRegistry)) {
      await loadRemotePlugins();
    }
  } catch (e) {
    showErr(`保存配置失败：${errMsg(e)}`);
  }
}

async function doExport(format: "yaml" | "json") {
  try {
    state.exportText = await run<string>("export_env", { format });
  } catch (e) {
    showErr(`导出失败：${errMsg(e)}`);
  }
}

async function doImport() {
  if (!state.importText.trim()) return showMsg("请先粘贴要导入的环境快照");
  try {
    await run<void>("import_env", { content: state.importText });
    showMsg("环境快照导入并应用成功");
    state.importText = "";
    state.exportText = null;
    await loadAll();
  } catch (e) {
    showErr(`导入失败：${errMsg(e)}`);
  }
}

// ---- 项目预设 ----
async function saveProjectPreset(p: ProjectPreset) {
  try {
    await run<void>("save_project", { preset: p });
    showMsg(`项目预设「${p.name}」已保存`);
    await loadProjects();
  } catch (e) {
    showErr(`保存项目预设失败：${errMsg(e)}`);
  }
}

async function deleteProjectPreset(name: string) {
  const ok = await confirmAsk(`删除项目预设「${name}」`, "不影响项目目录本身。");
  if (!ok) return;
  try {
    await run<void>("delete_project", { name });
    showMsg(`项目预设「${name}」已删除`);
    await loadProjects();
  } catch (e) {
    showErr(`删除项目预设失败：${errMsg(e)}`);
  }
}

async function launchProjectSession(name: string, command: string, args: string[]) {
  state.busy = `session-${name}`;
  try {
    await run<void>("launch_session", { project: name, command, args });
    showMsg(`已按「${name}」预设启动 ${command}：新窗口已打开，请到任务栏查看`);
  } catch (e) {
    showErr(`启动失败：${errMsg(e)}`);
  } finally {
    state.busy = null;
  }
}

// ---------- 导出 ----------
export function useApp() {
  return state;
}

export const store = {
  state,
  run,
  showMsg,
  showErr,
  loadTools,
  loadHome,
  loadAll,
  loadP2,
  loadVersions,
  install,
  switchVersion,
  unuseGlobal,
  uninstallVersion,
  uninstall,
  applyPreset,
  addCustomPreset,
  removeCustomPreset,
  saveProxy,
  cancelTask,
  cancelAll,
  clearFinished,
  ackConflict,
  fixToolConflict,
  installRemotePlugin,
  loadRemotePlugins,
  switchRemoteRegistry,
  toggleAutostart,
  toggleTrayResident,
  setToolMirror,
  currentToolMirror,
  addLuaPlugin,
  saveLuaPlugin,
  loadPluginSource,
  togglePlugin,
  deletePlugin,
  openPluginDir,
  saveSettings,
  doExport,
  doImport,
  refreshLogs,
  saveProjectPreset,
  deleteProjectPreset,
  launchProjectSession,
};
