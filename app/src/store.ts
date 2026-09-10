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
import { t } from "./i18n";

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
  if (e == null) return t("common.unknownError");
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
      positiveText: t("common.confirm"),
      negativeText: t("common.cancel"),
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
    { name: "官方gitee", url: "https://raw.giteeusercontent.com/envhive/envhive/raw/main/plugins/manifest.json" },
    { name: "官方github", url: "https://raw.githubusercontent.com/envhive/envhive/main/plugins/manifest.json" },
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
    showMsg(t("toast.previewMode", { msg: errMsg(e) }));
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
    showErr(t("toast.logReadFailed", { msg: errMsg(e) }));
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
          showMsg(t("toast.progressNote", { tool: p.tool, version: p.version, note: p.note }));
        }
      }
      if (p.stage === "done") {
        state.busy = null;
        const key = `${p.tool}|${p.version}`;
        window.setTimeout(() => {
          const cur = state.progress;
          if (cur && `${cur.tool}|${cur.version}` === key) state.progress = null;
        }, 1500);
        void notify(
          t("toast.installCompleteTitle"),
          t("toast.installCompleteBody", { tool: p.tool, version: p.version })
        );
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
      showErr(t("toast.installFailed", { msg: message }));
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
    const label = dist ? `${tool.display} · ${dist}` : tool.display;
    showMsg(t("toast.versionsRefreshed", { tool: label, count: versions.length }));
  } catch (e) {
    showErr(t("toast.versionsLoadFailed", { tool: tool.display, msg: errMsg(e) }));
  } finally {
    state.refreshing[key] = false;
  }
}

async function install(tool: ToolInfo) {
  const sel = state.selections[tool.name];
  const version = sel?.version;
  if (!version) return showMsg(t("toast.selectVersionFirst", { tool: tool.display }));
  state.busy = tool.name;
  try {
    const r = await run<QueueEnqueueResult>("enqueue_install", {
      name: tool.name,
      version,
      distribution: sel.dist ?? null,
    });
    if (r.reused) {
      showMsg(t("toast.alreadyQueued", { tool: tool.display, version, id: r.id }));
    } else {
      showMsg(t("toast.enqueued", { tool: tool.display, version, id: r.id }));
    }
    await loadQueue();
  } catch (e) {
    showErr(t("toast.enqueueFailed", { msg: errMsg(e) }));
  } finally {
    state.busy = null;
  }
}

async function switchVersion(tool: ToolInfo, version: string) {
  state.busy = tool.name;
  try {
    const r = await run<SwitchResult>("switch_version", { name: tool.name, version });
    showMsg(t("toast.switched", { tool: tool.display, version: r.version, message: r.message }));
    await Promise.all([loadTools(), loadQueue(), loadHome()]);
  } catch (e) {
    showErr(t("toast.switchFailed", { msg: errMsg(e) }));
  } finally {
    state.busy = null;
  }
}

/** 取消某工具的全局设置：解除激活并从 PATH / *_HOME 等环境变量移除（不卸载版本） */
async function unuseGlobal(tool: ToolInfo) {
  if (!tool.current) {
    showMsg(t("toast.notGlobalVersion", { tool: tool.display }));
    return;
  }
  const envName = tool.name.toUpperCase() + "_HOME";
  const ok = await confirmAsk(
    t("dialog.unuseGlobal.title", { tool: tool.display }),
    t("dialog.unuseGlobal.content", {
      tool: tool.display,
      version: tool.current,
      envName,
    })
  );
  if (!ok) return;
  state.busy = tool.name;
  try {
    await run<void>("unuse_global", { name: tool.name });
    showMsg(t("toast.globalCleared", { tool: tool.display }));
    await Promise.all([loadTools(), loadQueue(), loadHome()]);
  } catch (e) {
    showErr(t("toast.unuseFailed", { msg: errMsg(e) }));
  } finally {
    state.busy = null;
  }
}

async function uninstallVersion(tool: ToolInfo, version: string) {
  const ok = await confirmAsk(
    t("dialog.uninstall.title", { tool: tool.display, version }),
    t("dialog.uninstall.content")
  );
  if (!ok) return;
  state.busy = tool.name;
  try {
    await run<void>("uninstall_tool", { name: tool.name, version });
    showMsg(t("toast.uninstalled", { tool: tool.display, version }));
    await Promise.all([loadTools(), loadHome(), loadP2()]);
  } catch (e) {
    showErr(t("toast.uninstallFailed", { msg: errMsg(e) }));
  } finally {
    state.busy = null;
  }
}

async function uninstall(tool: ToolInfo) {
  const version = state.selections[tool.name]?.version || tool.current;
  if (!version) return showMsg(t("toast.noUninstallableVersion"));
  const ok = await confirmAsk(
    t("dialog.uninstall.title", { tool: tool.display, version }),
    t("dialog.uninstall.contentSelected")
  );
  if (!ok) return;
  state.busy = tool.name;
  try {
    await run<void>("uninstall_tool", { name: tool.name, version });
    showMsg(t("toast.uninstalled", { tool: tool.display, version }));
    await Promise.all([loadTools(), loadHome(), loadP2()]);
  } catch (e) {
    showErr(t("toast.uninstallFailed", { msg: errMsg(e) }));
  } finally {
    state.busy = null;
  }
}

// ---- 网络操作 ----
async function applyPreset(tool: string, name: string) {
  try {
    await run<void>("apply_registry", { tool, preset: name });
    showMsg(t("toast.mirrorApplied", { tool, name }));
    state.registry[tool] = await run<RegistryState>("get_registry_state", { tool });
  } catch (e) {
    showErr(t("toast.mirrorApplyFailed", { msg: errMsg(e) }));
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
    showMsg(t("toast.customPresetAdded", { tool, name }));
    await reloadToolPresets(tool);
  } catch (e) {
    showErr(t("toast.customPresetAddFailed", { msg: errMsg(e) }));
  }
}

async function removeCustomPreset(tool: string, name: string) {
  const ok = await confirmAsk(
    t("dialog.removeCustomPreset.title", { name }),
    t("dialog.removeCustomPreset.content", { tool, name })
  );
  if (!ok) return;
  try {
    await run<void>("remove_custom_registry_preset", { tool, name });
    showMsg(t("toast.customPresetRemoved", { tool, name }));
    await reloadToolPresets(tool);
  } catch (e) {
    showErr(t("toast.customPresetRemoveFailed", { msg: errMsg(e) }));
  }
}

async function saveProxy(silent = false) {
  try {
    await run<void>("set_proxy", { url: state.proxyUrl || null, enable: state.proxyEnable });
    if (!silent) {
      showMsg(
        state.proxyEnable
          ? t("toast.proxyEnabled", { url: state.proxyUrl })
          : t("toast.proxyDisabled")
      );
    }
    state.proxy = await run<ProxyConfig>("get_proxy");
  } catch (e) {
    showErr(t("toast.proxySaveFailed", { msg: errMsg(e) }));
  }
}

async function cancelTask(id: number) {
  try {
    await run<void>("cancel_task", { id });
    showMsg(t("toast.taskCancelled", { id }));
    await loadQueue();
  } catch (e) {
    showErr(t("toast.cancelFailed", { msg: errMsg(e) }));
  }
}

/** 全部取消：排队中直接取消，执行中请求取消（worker 收尾定终态） */
async function cancelAll() {
  const ok = await confirmAsk(t("dialog.cancelAll.title"), t("dialog.cancelAll.content"));
  if (!ok) return;
  try {
    const n = await run<number>("queue_cancel_all");
    showMsg(n > 0 ? t("toast.cancelRequested", { count: n }) : t("toast.nothingToCancel"));
    await loadQueue();
  } catch (e) {
    showErr(t("toast.cancelAllFailed", { msg: errMsg(e) }));
  }
}

/** 清空已完成/失败/已取消的任务，保留排队中与执行中 */
async function clearFinished() {
  try {
    const n = await run<number>("queue_clear_finished");
    showMsg(n > 0 ? t("toast.clearedFinished", { count: n }) : t("toast.nothingToClear"));
    await loadQueue();
  } catch (e) {
    showErr(t("toast.clearFailed", { msg: errMsg(e) }));
  }
}

// ---- 冲突 ----
async function ackConflict(c: ConflictInfo) {
  if (c.kind !== "registry") return;
  const ok = await confirmAsk(
    t("dialog.ackConflict.title"),
    t("dialog.ackConflict.content", { message: c.message })
  );
  if (!ok) return;
  try {
    await run<void>("ack_registry_conflict", { tool: c.tool });
    showMsg(t("toast.fingerprintRecorded", { tool: c.tool }));
    loadExtras();
  } catch (e) {
    showErr(t("toast.actionFailed", { msg: errMsg(e) }));
  }
}

async function fixToolConflict(c: ConflictInfo) {
  if (c.kind !== "tool" || !c.version) return;
  try {
    await run<SwitchResult>("switch_version", { name: c.tool, version: c.version });
    showMsg(t("toast.reswitched", { tool: c.tool, version: c.version }));
    await Promise.all([loadExtras(), loadTools(), loadHome()]);
  } catch (e) {
    showErr(t("toast.fixFailed", { msg: errMsg(e) }));
  }
}

async function installRemotePlugin(p: RemotePluginInfo) {
  // 同名插件已装且版本不同 → 视为「更新」（本地未声明版本的插件视为自定义，不提示更新）
  const local = state.plugins.find((x) => x.name === p.name);
  const prevVersion = local?.version ?? "";
  const isUpdate = !!prevVersion && prevVersion !== p.version;
  const ok = await confirmAsk(
    isUpdate
      ? t("dialog.installRemotePlugin.titleUpdate", {
          name: p.name,
          from: prevVersion,
          to: p.version,
        })
      : t("dialog.installRemotePlugin.titleInstall", { name: p.name, version: p.version }),
    p.description || t("common.noDescription")
  );
  if (!ok) return;
  try {
    // 整个插件对象传入：type / format / sha256 随包校验（后端 zip 安装链路）
    await run<void>("install_remote_plugin", { plugin: p });
    showMsg(
      isUpdate
        ? t("toast.pluginUpdated", { name: p.name, version: p.version })
        : t("toast.pluginInstalled", { name: p.name, version: p.version })
    );
    await Promise.all([loadTools(), loadExtras(), loadP2(), loadRemotePlugins()]);
  } catch (e) {
    showErr(
      isUpdate
        ? t("toast.pluginUpdateFailed", { msg: errMsg(e) })
        : t("toast.pluginInstallFailed", { msg: errMsg(e) })
    );
  }
}

// ---- P2 操作 ----
async function toggleAutostart() {
  state.busy = "autostart";
  try {
    const next = !state.autostart;
    await run<void>("set_autostart", { enable: next });
    state.autostart = next;
    showMsg(next ? t("toast.autostartOn") : t("toast.autostartOff"));
  } catch (e) {
    showErr(t("toast.autostartFailed", { msg: errMsg(e) }));
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
    showMsg(next ? t("toast.trayResidentOn") : t("toast.trayResidentOff"));
  } catch (e) {
    showErr(t("toast.trayResidentFailed", { msg: errMsg(e) }));
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
    showMsg(
      mirror
        ? t("toast.toolMirrorSet", { tool: tool.display, mirror })
        : t("toast.toolMirrorReset", { tool: tool.display })
    );
  } catch (e) {
    showErr(t("toast.toolMirrorFailed", { msg: errMsg(e) }));
  }
}

async function addLuaPlugin() {
  if (!state.luaName.trim() || !state.luaScript.trim()) return showMsg(t("toast.luaFieldsRequired"));
  try {
    await run<void>("add_lua_plugin", { name: state.luaName.trim(), script: state.luaScript });
    showMsg(t("toast.luaAdded", { name: state.luaName.trim() }));
    state.luaName = "";
    state.luaScript = "";
    await Promise.all([loadTools(), loadP2()]);
  } catch (e) {
    showErr(t("toast.luaAddFailed", { msg: errMsg(e) }));
  }
}

/** 更新已有插件源码（编辑模式；Lua 校验 + 原子写回） */
async function saveLuaPlugin(name: string, provider: string, script: string) {
  if (!script.trim()) return showMsg(t("toast.scriptEmpty"));
  try {
    await run<void>("save_plugin_source", { name, provider, script });
    showMsg(t("toast.pluginSaved", { name }));
    await Promise.all([loadTools(), loadP2()]);
  } catch (e) {
    showErr(t("toast.pluginSaveFailed", { msg: errMsg(e) }));
  }
}

/** 读取插件源码（编辑器加载） */
async function loadPluginSource(name: string): Promise<{ name: string; provider: string; script: string } | null> {
  try {
    return await run<{ name: string; provider: string; script: string }>("read_plugin_source", { name });
  } catch (e) {
    showErr(t("toast.pluginReadFailed", { msg: errMsg(e) }));
    return null;
  }
}

/** 禁用 / 启用插件 */
async function togglePlugin(name: string, enable: boolean) {
  try {
    await run<void>("toggle_plugin", { name, enable });
    showMsg(
      enable ? t("toast.pluginEnabled", { name }) : t("toast.pluginDisabled", { name })
    );
    await Promise.all([loadTools(), loadP2()]);
  } catch (e) {
    showErr(
      enable
        ? t("toast.pluginEnableFailed", { msg: errMsg(e) })
        : t("toast.pluginDisableFailed", { msg: errMsg(e) })
    );
  }
}

/** 删除插件（确认由调用方弹出；仅删插件定义，已安装版本保留） */
async function deletePlugin(name: string, display: string) {
  const ok = await confirmAsk(
    t("dialog.deletePlugin.title", { name }),
    t("dialog.deletePlugin.content", { name, display })
  );
  if (!ok) return;
  try {
    await run<void>("delete_plugin", { name });
    showMsg(t("toast.pluginDeleted", { name }));
    await Promise.all([loadTools(), loadP2()]);
  } catch (e) {
    showErr(t("toast.pluginDeleteFailed", { msg: errMsg(e) }));
  }
}

/** 在系统文件管理器中打开插件目录 */
async function openPluginDir(name: string) {
  try {
    await run<void>("open_plugin_dir", { name });
  } catch (e) {
    showErr(t("toast.pluginDirFailed", { msg: errMsg(e) }));
  }
}

async function saveSettings(silent = false) {
  const entries = state.registryEntries
    .map((e) => ({ name: (e.name ?? "").trim(), url: (e.url ?? "").trim() }))
    .filter((e) => e.url);
  if (entries.length === 0) return showErr(t("toast.registryRequired"));
  const oldRegistry = state.remoteRegistry;
  try {
    await run<void>("update_config", {
      cacheTtl: state.cacheTtl.trim() || null,
      registryEntries: entries,
      storagePath: state.storagePath.trim() || null,
    });
    if (!silent) showMsg(t("toast.configSaved"));
    await loadSettings();
    // 当前选中地址被删除时，回退到新列表并刷新插件市场
    if (!entries.some((e) => e.url === oldRegistry)) {
      await loadRemotePlugins();
    }
  } catch (e) {
    showErr(t("toast.configSaveFailed", { msg: errMsg(e) }));
  }
}

async function doExport(format: "yaml" | "json") {
  try {
    state.exportText = await run<string>("export_env", { format });
  } catch (e) {
    showErr(t("toast.exportFailed", { msg: errMsg(e) }));
  }
}

async function doImport() {
  if (!state.importText.trim()) return showMsg(t("toast.importRequired"));
  try {
    await run<void>("import_env", { content: state.importText });
    showMsg(t("toast.imported"));
    state.importText = "";
    state.exportText = null;
    await loadAll();
  } catch (e) {
    showErr(t("toast.importFailed", { msg: errMsg(e) }));
  }
}

// ---- 项目预设 ----
async function saveProjectPreset(p: ProjectPreset) {
  try {
    await run<void>("save_project", { preset: p });
    showMsg(t("toast.projectSaved", { name: p.name }));
    await loadProjects();
  } catch (e) {
    showErr(t("toast.projectSaveFailed", { msg: errMsg(e) }));
  }
}

async function deleteProjectPreset(name: string) {
  const ok = await confirmAsk(
    t("dialog.deleteProject.title", { name }),
    t("dialog.deleteProject.content")
  );
  if (!ok) return;
  try {
    await run<void>("delete_project", { name });
    showMsg(t("toast.projectDeleted", { name }));
    await loadProjects();
  } catch (e) {
    showErr(t("toast.projectDeleteFailed", { msg: errMsg(e) }));
  }
}

async function launchProjectSession(name: string, command: string, args: string[]) {
  state.busy = `session-${name}`;
  try {
    await run<void>("launch_session", { project: name, command, args });
    showMsg(t("toast.sessionLaunched", { name, command }));
  } catch (e) {
    showErr(t("toast.sessionLaunchFailed", { msg: errMsg(e) }));
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
