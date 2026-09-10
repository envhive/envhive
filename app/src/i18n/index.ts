// i18n —— 前端多语言统一入口
//
// 规范见 docs/frontend-i18n-spec.md。三条硬约束：
//   1. 组件内用 `useI18n()` 的 t（响应式）；组件外（store / 工具模块）用本文件导出的 `t`。
//   2. 语言包只有 zh-CN / en-US 两份，key 结构由 `typeof zhCN` 强约束，缺 key 直接编译报错。
//   3. 插值一律用命名参数 `{name}`，禁止代码侧拼接可本地化片段。
import { computed } from "vue";
import { createI18n } from "vue-i18n";
import {
  zhCN as naiveZhCN,
  dateZhCN as naiveDateZhCN,
  enUS as naiveEnUS,
  dateEnUS as naiveDateEnUS,
} from "naive-ui";
import type { NDateLocale, NLocale } from "naive-ui";
import zhCN from "./locales/zh-CN";
import enUS from "./locales/en-US";
import type { DownloadStage, QueueTask } from "../types";

/** 语言包基准类型（以 zh-CN 为准，en-US 需完全对齐） */
export type MessageSchema = typeof zhCN;

// 全局 key 类型推断：让 `t()` / `$t()` 的 key 受语言包约束，写错 key 或漏 key 都会编译报错。
declare module "vue-i18n" {
  export interface DefineLocaleMessage extends MessageSchema {}
}

/** 支持的语言：key 为 BCP-47 标签；label 恒用该语言自身写法（不随当前语言翻译） */
export const LOCALES = [
  { key: "zh-CN", label: "简体中文" },
  { key: "en-US", label: "English" },
] as const;

export type LocaleKey = (typeof LOCALES)[number]["key"];

/** 默认语言（同时作为 fallbackLocale） */
export const DEFAULT_LOCALE: LocaleKey = "zh-CN";

/** localStorage 持久化键 */
const STORAGE_KEY = "envhive.locale";

function isLocaleKey(v: unknown): v is LocaleKey {
  return LOCALES.some((l) => l.key === v);
}

/** 首选语言：上次选择 → 浏览器语言命中 → 默认 */
function resolveInitialLocale(): LocaleKey {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (isLocaleKey(saved)) return saved;
    const nav = navigator.language;
    if (isLocaleKey(nav)) return nav;
    // zh-Hans-CN / en-GB 等区域变体：按主语言标签回退
    const primary = nav?.split("-")[0];
    const hit = LOCALES.find((l) => l.key.split("-")[0] === primary);
    if (hit) return hit.key;
  } catch {
    /* 无 localStorage / navigator：忽略，用默认语言 */
  }
  return DEFAULT_LOCALE;
}

export const i18n = createI18n({
  legacy: false,
  globalInjection: true,
  locale: resolveInitialLocale(),
  fallbackLocale: DEFAULT_LOCALE,
  messages: { "zh-CN": zhCN, "en-US": enUS },
});

/** 插值参数：仅接受标量，避免把对象 / 数组塞进文案 */
export type TParams = Record<string, string | number>;

/**
 * 组件外翻译（store、事件回调、非 setup 上下文）。
 * 每次调用都读取当前 locale，因此始终返回最新语言文案；
 * 组件内请用 `useI18n().t` 以获得响应式刷新。
 */
export function t(key: string, params?: TParams): string {
  const translate = i18n.global.t as unknown as (k: string, p?: TParams) => string;
  return params ? translate(key, params) : translate(key);
}

/** 当前语言（响应式，供语言切换器 / NConfigProvider 使用） */
export const currentLocale = computed<LocaleKey>(() => i18n.global.locale.value as LocaleKey);

/** 切换语言：更新实例 + 持久化 + 同步 <html lang> */
export function setLocale(locale: LocaleKey): void {
  i18n.global.locale.value = locale;
  try {
    localStorage.setItem(STORAGE_KEY, locale);
  } catch {
    /* 忽略持久化失败 */
  }
  if (typeof document !== "undefined") document.documentElement.lang = locale;
}

// ---------- Naive UI 组件库语言桥接 ----------
const NAIVE_LOCALES: Record<LocaleKey, { locale: NLocale; dateLocale: NDateLocale }> = {
  "zh-CN": { locale: naiveZhCN, dateLocale: naiveDateZhCN },
  "en-US": { locale: naiveEnUS, dateLocale: naiveDateEnUS },
};

export const naiveLocale = computed<NLocale>(() => NAIVE_LOCALES[currentLocale.value].locale);
export const naiveDateLocale = computed<NDateLocale>(
  () => NAIVE_LOCALES[currentLocale.value].dateLocale
);

// ---------- 枚举 → 文案（key 与后端枚举值一一对应，见规范「枚举映射」节） ----------
/** 下载阶段文案（DownloadProgress.stage） */
export function stageLabel(stage: DownloadStage): string {
  return t(`stage.${stage}`);
}

/** 队列任务状态文案（QueueTask.status） */
export function taskStatusLabel(status: QueueTask["status"]): string {
  return t(`task.${status}`);
}
