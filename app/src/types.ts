// 全部接口类型（UI v2：从 App.tsx 抽出，与后端 serde camelCase 对齐）
import type {Component} from "vue";
import {
    HomeOutline,
    GlobeOutline,
    SettingsOutline,
    BarChartOutline,
    DocumentTextOutline,
    InformationCircleOutline, ExtensionPuzzleOutline, GridOutline,
} from "@vicons/ionicons5";

// ----------工具基础 ----------
// 发行商维度（Lua 插件 TOOL.distributions）
export interface DistributionInfo {
    key: string;
    display: string;
}

// 下载加速镜像候选（Lua 插件 TOOL.mirrors；from 前缀命中即替换为 to）
export interface MirrorCandidate {
    name: string;
    from: string;
    to: string;
}

export interface ToolInfo {
    name: string;
    display: string;
    category: string;
    homepage: string;
    current: string | null;
    installed: string[];
    available: string[] | null;
    binPath: string | null;
    // 声明则渲染「发行商」下拉；无则仅「版本」下拉
    distributions?: DistributionInfo[];
    defaultDistribution?: string | null;
    // 声明则渲染「加速镜像」下拉（多地址可切换）
    mirrors?: MirrorCandidate[];
    defaultMirror?: string | null;
    /**工具图标 data URI（`data:<mime>;base64,...`；无图标为 null/undefined，前端回退彩色圆点） */
    icon?: string | null;
}

// 工具选择 = 发行商 + 版本（无发行商维度的工具无 dist）
export interface ToolSelection {
    dist?: string;
    version: string;
}

export interface SwitchResult {
    tool: string;
    version: string;
    binPath: string;
    message: string;
}

export interface DownloadProgress {
    tool: string;
    version: string;
    percent: number;
    speedMbps: number;
    stage: "resolving" | "downloading" | "verifying" | "extracting" | "done" | "failed";
    /** 实际下载地址（镜像替换后的最终 URL；下载阶段有效，其余阶段可能为空） */
    url?: string;
    /** 附加提示（如「镜像下载失败，已回退官方源」；无则缺省） */
    note?: string;
    /** 下载文件总大小（字节；服务器未提供 Content-Length 时为 undefined） */
    totalBytes?: number;
    /** 已下载字节数（断点续传时含此前已下载部分） */
    downloadedBytes?: number;
}

export interface QueueTask {
    id: number;
    tool: string;
    version: string;
    distribution?: string | null;
    status: "queued" | "running" | "done" | "failed" | "cancelled";
    percent: number;
    message: string | null;
    /** 入队时间（epoch 秒；后端下发） */
    createdAt: number;
    /** 终态时间（epoch 秒；终态任务有效） */
    finishedAt?: number | null;
    /** 前端合并 download-progress 的实时阶段（running 时有效；后端 queue-updated 不携带） */
    stage?: DownloadStage;
    /** 前端合并 download-progress 的实时下载速度（MB/s） */
    speedMbps?: number;
    /** 前端合并 download-progress 的实际下载地址（下载阶段有效） */
    url?: string;
    /** 前端合并 download-progress 的下载总大小（字节；未知则缺省） */
    totalBytes?: number;
    /** 前端合并 download-progress 的已下载字节数 */
    downloadedBytes?: number;
}

/** enqueue_install 返回：reused=true 表示队列中已有同任务，未重复入队 */
export interface QueueEnqueueResult {
    id: number;
    reused: boolean;
}

export type DownloadStage = DownloadProgress["stage"];

// ---------- 网络：镜像 / 代理 ----------
export interface RegistryState {
    tool: string;
    currentUrl: string | null;
    configFile: string | null;
    presetName: string | null;
}

export interface PresetInfo {
    tool: string;
    name: string;
    url: string;
    isOfficial: boolean;
    /// 是否为用户自定义源（网络页可追加/删除）
    isCustom?: boolean;
}

export interface ProxyConfig {
    url: string | null;
    enable: boolean;
}

export interface DownloadMirrorConfig {
    enable: boolean;
    rules: Record<string, string>;
}

export interface ConflictInfo {
    kind: "registry" | "tool";
    tool: string;
    message: string;
    path: string | null;
    detail: string | null;
    version?: string | null;
}

// ---------- 首页总览 ----------
export interface EnvKeyValue {
    key: string;
    value: string;
}

export interface ToolEnvInfo {
    name: string;
    display: string;
    category: string;
    version: string | null;
    active: boolean;
    vars: EnvKeyValue[];
    pathEntries: string[];
    /**工具图标 data URI（无图标为 null/undefined，前端回退彩色圆点） */
    icon?: string | null;
}

export interface EnvVarRow {
    key: string;
    value: string;
    source: string;
}

export interface HomeOverview {
    tools: ToolEnvInfo[];
    merged: EnvVarRow[];
}

// UI v2：应用为系统环境变量结果
export interface ApplyGlobalResult {
    applied: boolean;
    message: string;
}

// UI v2：项目预设（首页「项目环境」区）
export interface ProjectToolVersion {
    tool: string;
    distribution?: string | null;
    version: string;
}

export interface ProjectPreset {
    name: string;
    dir: string;
    versions: ProjectToolVersion[];
}

// ---------- 更新 / 插件 ----------
export interface RemotePluginInfo {
    name: string;
    version: string;
    description: string;
    /** 插件包下载地址（manifest 内可为相对路径，如 plugins/go.zip，后端按 manifest.json 所在目录解析） */
    downloadUrl: string;
    /** 插件类型：lua */
    type?: string;
    /** 分发格式：zip / file（直链文本，缺省 file） */
    format?: string;
    /** zip 包 sha256 校验值（format=zip 时后端强制校验） */
    sha256?: string | null;
    /** zip 包字节数（可选） */
    size?: number | null;
    /** 插件主页（可选） */
    homepage?: string;
    /** 插件图标 data URI（市场 manifest 可携带；无图标为 null/undefined） */
    icon?: string | null;
}

export interface PluginInfo {
    name: string;
    display: string;
    category: string;
    homepage: string;
    provider: string;
    /** 插件自身版本号（插件定义未声明时为 null；用于市场「有新版本可更新」判断） */
    version: string | null;
    installedVersions: string[];
    /** 来源：market（远程 Git 仓库安装）/ local（本地创建） */
    source: "market" | "local";
    /** 发行商维度（TOOL.distributions 声明） */
    distributions: DistributionInfo[];
    /** 插件目录路径（真实名目录，不含 .disabled 后缀） */
    path: string;
    /** 插件定义文件最后修改时间（epoch 秒） */
    updatedAt: number | null;
    /** 是否启用（false = 目录为 <name>.disabled） */
    enabled: boolean;
    /**工具图标 data URI（无图标为 null/undefined，前端回退彩色圆点） */
    icon?: string | null;
}

export interface PluginSource {
    name: string;
    /** 插件类型：lua */
    provider: string;
    script: string;
}

// ---------- 设置 ----------
/** 远程插件仓库条目：name = 仓库名（插件市场下拉显示），url = manifest.json 完整地址 */
export interface RegistryEntry {
    name: string;
    url: string;
}

export interface BootstrapInfo {
    version: string;
    platform: string;
    arch: string;
    installDir: string;
    toolsDir: string;
    logsDir: string;
    /** 全局配置文件真实路径（~/.envhive/config.yaml） */
    configFile: string;
    configWritable: boolean;
}

export interface SettingsConfig {
    cache: { availableHookDuration: string };
    /**
     * 多市场注册表：addresses 为仓库列表，兼容旧版纯字符串地址数组（自动命名为
     * 「官方gitee / 官方github / <host> 仓库」）；selected 为插件市场上次切换选中的
     * 仓库 URL（启动时默认展示，不在列表中回退第一个）；address 兼容旧版单地址（读取兜底用）。
     */
    registry: { addresses: (RegistryEntry | string)[]; address?: string; selected?: string };
    storage: { toolPath: string };
    proxy: ProxyConfig;
    downloadMirror: DownloadMirrorConfig;
    autostart: { enable: boolean };
    env: Record<string, string>;
}

// ---------- 统计 ----------
export interface UsageStats {
    totalVersions: number;
    totalDiskBytes: number;
    toolUsage: ToolUsage[];
}

export interface ToolUsage {
    tool: string;
    display: string;
    totalCount: number;
    diskBytes: number;
    versions: VersionUsage[];
    /**工具图标 data URI（无图标为 null/undefined，前端回退彩色圆点） */
    icon?: string | null;
}

export interface VersionUsage {
    version: string;
    count: number;
    firstUsedAt: number | null;
    lastUsedAt: number | null;
    lastUsedDaysAgo: number | null;
    diskBytes: number;
    installed: boolean;
    isCurrent: boolean;
    projects: { project: string; hits: number }[];
}

// ---------- 日志 ----------
export interface LogFileInfo {
    name: string;
    size: number;
    modifiedAt: number;
}

// ---------- 导航 ----------
// UI v2：8 页（9 页平铺收敛 + 关于）
export type PageKey = "home" | "tools" | "network" | "plugins" | "settings" | "stats" | "logs" | "about";

export interface NavItem {
    key: PageKey;
    label: string;
    /** 图标组件（@vicons/ionicons5 的 Vue SVG 组件） */
    icon: Component;
}

// 导航：全部平铺，不分组（图标统一使用 ionicons5 Outline 风格，随菜单着色）
export const NAV_ITEMS: NavItem[] = [
    {key: "home", label: "首页", icon: HomeOutline},
    {key: "tools", label: "工具管理", icon: GridOutline},
    {key: "plugins", label: "插件", icon: ExtensionPuzzleOutline},
    {key: "network", label: "镜像源管理", icon: GlobeOutline},
    {key: "settings", label: "设置", icon: SettingsOutline},
    {key: "stats", label: "统计", icon: BarChartOutline},
    {key: "logs", label: "日志", icon: DocumentTextOutline},
    {key: "about", label: "关于", icon: InformationCircleOutline},
];

// ---------- 常量 ----------
export const REGISTRY_TOOLS = ["npm", "pip", "cargo", "maven", "go", "docker", "nuget", "gem", "pub", "conda"];
export const REGISTRY_TOOL_HINT: Record<string, string> = {
    npm: "~/.npmrc",
    pip: "pip.ini / pip.conf",
    cargo: "~/.cargo/config.toml",
    maven: "~/.m2/settings.xml",
    go: "go env -w GOPROXY",
    docker: "~/.docker/config.json",
    nuget: "NuGet.Config",
    gem: "~/.gemrc",
    pub: "~/.pub-cache/config.json",
    conda: "~/.condarc",
};

export const DOT_COLORS = ["#d4537e", "#639922", "#378add", "#ef9f27"];

export const STAGE_TEXT: Record<string, string> = {
    resolving: "解析版本",
    downloading: "下载中",
    verifying: "校验中",
    extracting: "解压安装",
    done: "完成",
    failed: "失败",
};

export const TASK_STATUS_TEXT: Record<string, string> = {
    queued: "排队中",
    running: "执行中",
    done: "完成",
    failed: "失败",
    cancelled: "已取消",
};

// ----------工具函数 ----------
export function versionsKey(tool: string, dist?: string): string {
    return dist ? `${tool}-${dist}` : tool;
}

export function lastNonFx(versions: string[]): string {
    if (versions.length === 0) return "";
    const nonFx = versions.filter((v) => !v.includes(".fx-"));
    return (nonFx[nonFx.length - 1] ?? versions[versions.length - 1]) || "";
}

// ---------- semver 比较（容忍 +build / -zulu / -fx 后缀） ----------
function versionParts(v: string): (number | string)[] {
    return v.split(/[.+-]/).map((s) => (/^\d+$/.test(s) ? parseInt(s, 10) : s));
}

export function compareVersions(a: string, b: string): number {
    const pa = versionParts(a);
    const pb = versionParts(b);
    const n = Math.max(pa.length, pb.length);
    for (let i = 0; i < n; i++) {
        const x = pa[i];
        const y = pb[i];
        if (x === undefined) return -1;
        if (y === undefined) return 1;
        if (x === y) continue;
        if (typeof x === "number" && typeof y === "number") return x - y;
        if (typeof x === "number") return -1;
        if (typeof y === "number") return 1;
        return x < y ? -1 : 1;
    }
    return 0;
}

export function fmtBytes(bytes: number): string {
    if (!bytes) return "0 B";
    const units = ["B", "KB", "MB", "GB", "TB"];
    let i = 0;
    let v = bytes;
    while (v >= 1024 && i < units.length - 1) {
        v /= 1024;
        i++;
    }
    return `${v.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

/** 下载速度格式化：< 1 MB/s 显示 KB/s（取整），>= 1 MB/s 显示 MB/s（1 位小数） */
export function fmtSpeed(mbps: number): string {
    if (!mbps || Number.isNaN(mbps)) return "0 KB/s";
    if (mbps < 1) return `${Math.round(mbps * 1024)} KB/s`;
    return `${mbps.toFixed(1)} MB/s`;
}

/** 秒 → 人类可读时长（"42s" / "1m 24s" / "1h 5m"）；<=0 返回空串 */
export function fmtDurationSec(sec: number): string {
    if (!sec || Number.isNaN(sec) || sec <= 0) return "";
    if (sec < 60) return `${Math.max(1, Math.round(sec))}s`;
    const m = Math.floor(sec / 60);
    if (m < 60) {
        const s = Math.round(sec % 60);
        return s > 0 ? `${m}m ${s}s` : `${m}m`;
    }
    const h = Math.floor(m / 60);
    const rm = m % 60;
    return rm > 0 ? `${h}h ${rm}m` : `${h}h`;
}

/** 任务耗时（epoch 秒差值）：start → end（end 缺省时返回空串） */
export function fmtDuration(startSec: number, endSec?: number | null): string {
    if (endSec == null) return "";
    return fmtDurationSec(Math.max(0, endSec - startSec));
}

/** epoch 秒 → 相对时间（"3 天前"）；超过 30 天显示日期 */
export function relTime(ts: number | null | undefined): string {
    if (!ts) return "";
    const diff = Date.now() / 1000 - ts;
    if (diff < 60) return "刚刚";
    if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
    if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
    if (diff < 30 * 86400) return `${Math.floor(diff / 86400)} 天前`;
    const d = new Date(ts * 1000);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** epoch 秒 → 完整日期时间（tooltip 用） */
export function fmtDateTime(ts: number | null | undefined): string {
    if (!ts) return "";
    const d = new Date(ts * 1000);
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
