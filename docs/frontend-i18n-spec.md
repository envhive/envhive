# 前端多语言（i18n）规范

> 状态：**全局层已落地**（导航 / 公共提示 / 状态枚举 / 对话框），页面与业务组件文案待按本规范增量迁移。
> 适用范围：`app/src`（Vue 3 + Naive UI + TypeScript）。技术栈：`vue-i18n@11`（Composition API 模式）。

---

## 0. 结论先行

| 项 | 约定 |
| --- | --- |
| 库 | `vue-i18n` v11，`legacy: false`（Composition API），开启 `globalInjection` |
| 语言 | `zh-CN`（默认 / fallback）、`en-US` |
| 入口 | `src/i18n/index.ts`（实例、切换、Naive UI 桥接、`t()`） |
| 语言包 | `src/i18n/locales/zh-CN.ts`（**key 基准**）、`locales/en-US.ts` |
| 组件内取词 | `const { t } = useI18n()`（响应式） |
| 组件外取词 | `import { t } from "@/i18n"`（store / 事件回调） |
| 类型安全 | `zh-CN` 导出结构即 schema；`en-US` 用 `typeof zhCN` 约束；并做 `DefineLocaleMessage` 增强 |
| 唯一事实来源 | 状态枚举不再在 `types.ts` 维护映射表，统一收敛到 `stage.*` / `task.*` |

---

## 1. 目录与文件

```
app/src/i18n/
├─ index.ts              # createI18n / locale 持久化 / Naive UI locale 桥接 / t() / 枚举取词
└─ locales/
   ├─ zh-CN.ts           # 简体中文（默认语言，key 基准）
   └─ en-US.ts           # English (US)
```

**硬约束**

1. 语言包只允许 `zh-CN` / `en-US` 两份，**key 结构必须完全一致**（由 `typeof zhCN` 类型注解强制，缺 key / 多 key 直接编译报错）。
2. 禁止在组件里维护任何「中文 → 文案」的本地映射表（如旧的 `STAGE_TEXT` / `TASK_STATUS_TEXT`），一律走语言包。
3. 新增语言只需：加 `locales/<tag>.ts` → `LOCALES` 增项 → `messages` 注册 → `NAIVE_LOCALES` 补 Naive UI 语言对象。

---

## 2. 命名空间与 key 命名

**规则**

- key 用点号路径，全小写，末段 camelCase：`<命名空间>.<语义>`。
- 命名空间按「页面 / 模块」聚合，不按控件类型聚合。
- 同一文案被 ≥2 处引用时必须上位到公共命名空间（`common.*`），禁止复制。

| 命名空间 | 用途 | 示例 |
| --- | --- | --- |
| `app.*` | 应用壳（启动占位等） | `app.loading.title` |
| `nav.*` | 侧边导航 / 顶栏标题（**key 与 `PageKey` 一一对应**） | `nav.tools` |
| `common.*` | 跨模块通用动作与占位 | `common.confirm`、`common.retry` |
| `language.*` | 语言切换器 | `language.label` |
| `topbar.*` | 顶栏 | `topbar.storageReadonly` |
| `stage.*` | 下载阶段（`DownloadProgress.stage`） | `stage.downloading` |
| `task.*` | 队列任务状态（`QueueTask.status`） | `task.queued` |
| `queue.*` | 下载队列抽屉 | `queue.cancelAll` |
| `install.*` | 安装弹窗 | `install.installVersion` |
| `toast.*` | 全局提示 / 系统通知 | `toast.enqueued` |
| `dialog.*` | 确认对话框（`title` / `content` 成对） | `dialog.uninstall.title` |
| `time.*` | 相对时间（`relTime`） | `time.daysAgo` |
| `<page>.*` | 页面私有文案 | `tools.listTitle` |
| `<module>.*` | 跨页复用的业务组件（非页面） | `toolCard.installedVersions`、`vcp.cloneGlobal` |

> 页面命名空间（已全部落地）：`home.*` / `tools.*` / `plugins.*` / `network.*` / `settings.*` / `stats.*` / `logs.*` / `about.*`。
>
> 业务组件命名空间：`toolCard.*`（`ToolCard.vue`）/ `project.*`（`ProjectEnvRow.vue`）/ `vcp.*`（`VersionComboPicker.vue`）/ `envTable.*`（`EnvTable.vue`）。

---

## 3. 取词方式

**组件内**（响应式，语言切换后自动刷新）：

```ts
import { useI18n } from "vue-i18n";
const { t } = useI18n();
```
```vue
<span>{{ t("queue.title") }}</span>
```

**组件外**（store、事件回调、非 setup 上下文）：

```ts
import { t } from "../i18n";
showMsg(t("toast.enqueued", { tool, version, id }));
```

> ⚠️ 模板里 **不要** 用 `t` 当循环变量（`v-for="t in list"`）——会遮蔽翻译函数。本仓库约定：
> 一律用驼峰语义名（`task` / `item` / `entry`），翻译函数固定叫 `t`。

---

## 4. 插值、复数与格式化

- **只允许命名插值**：`"{tool} {version} 已加入队列（任务 #{id}）"`，调用侧 `t("toast.enqueued", { tool, version, id })`。
- **禁止字符串拼接**：不要把可本地化片段用 `+` / 模板串拼起来——词序、标点、量词在不同语言里都会变。
  - ❌ `` showMsg(`${tool.display} 已取消全局设置`) ``
  - ✅ `showMsg(t("toast.globalCleared", { tool: tool.display }))`
- **纯技术标识符可以拼接**（版本号、URL、任务编号之外的分隔符），但拼接结果只作为**参数**传入文案：
  ```ts
  const label = dist ? `${tool.display} · ${dist}` : tool.display;
  showMsg(t("toast.versionsRefreshed", { tool: label, count: versions.length }));
  ```
- vue-i18n 的复数（`|` 管道）本期未使用：中文无单复数，英文侧优先用中性措辞（如 `{count} tasks`）规避；确有必要时再引入 `plural` 语法并在本文档登记。

---

## 5. 枚举映射

后端枚举值**就是** key 尾段，新增状态只需补语言包，不再维护映射表。

| 后端字段 | 取值 | key |
| --- | --- | --- |
| `DownloadProgress.stage` | `resolving` / `downloading` / `verifying` / `extracting` / `done` / `failed` | `stage.<value>` |
| `QueueTask.status` | `queued` / `running` / `done` / `failed` / `cancelled` | `task.<value>` |

取词统一走 `src/i18n` 导出的类型化辅助函数（参数为字面量联合，写错取值直接编译报错）：

```ts
import { stageLabel, taskStatusLabel } from "../i18n";
stageLabel(task.stage);        // stage.<value>
taskStatusLabel(task.status);  // task.<value>
```

> 旧代码：`types.ts` 已删除 `STAGE_TEXT` / `TASK_STATUS_TEXT`，请勿再新增类似的本地文案表。

---

## 6. 类型安全与校验

- `src/i18n/index.ts` 中通过 `declare module "vue-i18n" { interface DefineLocaleMessage extends MessageSchema {} }` 做全局 key 增强：`t("…")` 的 key 受语言包约束。
- `en-US.ts` 顶部 `const enUS: typeof zhCN = { … }`：两侧 key 必须严格对齐（缺 key / 多余 key 都是 TS 错误）。
- 组件外 `t(key: string, params?)` 的 key 是动态字符串，编译器无法校验——**新增 key 后请手动确认两份语言包都已补**。

---

## 7. 语言切换与持久化

| 项 | 实现 |
| --- | --- |
| 默认语言 | `zh-CN` |
| 持久化 key | `localStorage["envhive.locale"]` |
| 首次启动 | 上次选择 → `navigator.language` 命中（含 `zh-Hans-CN` 这类变体回退主语言）→ 默认 |
| 切换入口 | 顶栏右侧（`TopBar.vue`）语言下拉，`setLocale()` |
| 组件库联动 | `naiveLocale` / `naiveDateLocale` 计算属性绑定 `NConfigProvider`，与 vue-i18n 同步 |
| DOM 联动 | `setLocale()` 同步 `<html lang>` |

---

## 8. 不纳入 i18n 的内容

以下属于**数据层 / 字标**，不进语言包：

1. **后端返回的文案**：`QueueTask.message`、`ConflictInfo.message`、`SwitchResult.message`（安装 / 冲突 / 切换结果，由 Rust 侧产出）。
2. **插件声明的展示名**：`TOOL.display`、`TOOL.distributions[].display`（如“官方版”“Eclipse Temurin”）。
3. **品牌字标**：侧边栏 `蜂巢` / `EnvHive` 为 logo 锁定组合，保持静态。
4. **仓库显示名**：`registryName()` 生成的「官方gitee / 官方github / <host> 仓库」，需与后端 `auto_registry_name` 规则一致。
5. **DEV mock 数据**：`store.ts` 的 `PREVIEW_TOOLS` / `LUA_SAMPLE`（仅未连后端时的浏览器预览）。
6. **代码注释与 `console.warn` 日志**。

> 若后续要让第 1 项也支持多语言，应由后端随事件下发语言相关的消息 key（或错误码），前端再做映射——**不要**把后端文案强行拆进语言包。

---

## 9. SOP

**新增文案 key**

1. 在 `locales/zh-CN.ts` 的对应命名空间下加 key（中文文案）。
2. 同步在 `locales/en-US.ts` 补同 key 英文文案。
3. 组件内用 `useI18n().t` / 组件外用 `import { t }`。
4. **若文案位于模块级常量表**（如 `const MAP = {...}`、`const COLS = [...]`），不要直接 `t()` 求值存入——它只在 setup 时求值一次，切语言不会刷新。改为：常量存 key（模板里 `t(item.key)`），或把常量改成函数 / `computed`。
5. 跑 `npm run build`（= `vue-tsc --noEmit` + `vite build`）确认零错误。

**新增语言**（如 `ja-JP`）

1. 新建 `locales/ja-JP.ts`：`const jaJP: typeof zhCN = { … }`。
2. `index.ts`：`LOCALES` 增项、`messages` 注册、`NAIVE_LOCALES` 补 Naive UI 的 `jaJP` + `dateJaJP`。
3. `resolveInitialLocale()` 的 `navigator.language` 命中逻辑自动生效，无需改动。

---

## 10. 迁移进度

### 已完成（全局层）

| 文件 | 覆盖内容 |
| --- | --- |
| `src/i18n/**` | 新建：实例、语言包、切换、Naive UI 桥接、枚举取词 |
| `src/main.ts` | 注册 `app.use(i18n)` |
| `src/App.vue` | `NConfigProvider` 语言绑定 + 启动占位文案 |
| `src/components/Sidebar.vue` | 导航 label 改为 `t(i18nKey)`（响应式） |
| `src/components/TopBar.vue` | 页面标题 + 存储徽标 + **语言切换器** |
| `src/components/QueueDrawer.vue` | 全量文案 + 枚举取词 |
| `src/components/InstallToolModal.vue` | 全量文案 + 删除重复 `STAGE_TEXT` |
| `src/store.ts` | 全部 `showMsg` / `showErr` / `confirmAsk` / `notify` 文案 |
| `src/hooks/useBackend.ts` | 下载失败事件提示 |
| `src/types.ts` | `NavItem.label` → `i18nKey`；删除 `STAGE_TEXT` / `TASK_STATUS_TEXT` |

### 已完成（页面 / 业务组件）

| 文件 | 命名空间 | 备注 |
| --- | --- | --- |
| `pages/HomePage.vue` | `home.*` | 含 `categoryLabel()` 枚举映射、切换 Popselect 提示 |
| `pages/ToolsPage.vue` | `tools.*` | 分类筛选哨兵值由 `"全部"` 改为 `""`，标签走 `t()` |
| `pages/PluginsPage.vue` | `plugins.*` | `SRC_META` 常量表 → `srcMeta()` 函数（常量表不随语言刷新） |
| `pages/NetworkPage.vue` | `network.*` | 预设 label 的「（官方）/（自定义）」后缀也走语言包 |
| `pages/SettingsPage.vue` | `settings.*` | 含关于卡片的 `{version}`/`{platform}` 参数化 |
| `pages/StatsPage.vue` | `stats.*` | `DataTableColumns` 在 `computed` 内取词（随语言刷新） |
| `pages/LogsPage.vue` | `logs.*` | `levelOptions` 常量 → `computed` |
| `pages/AboutPage.vue` | `about.*` | `TECH_ARCH` 存 key 而非文案；`LICENSE.display` 取词 |
| `components/ToolCard.vue` | `toolCard.*` | — |
| `components/VersionComboPicker.vue` | `vcp.*` | — |
| `components/ProjectEnvRow.vue` | `project.*` + `common.*` | — |
| `components/EnvTable.vue` | `envTable.*` + `common.copy` | — |
| `components/ToolIcon.vue` | `common.toolIconAlt` | 无障碍 alt 文本 |
| `types.ts` `relTime()` | `time.*` | 相对时间；`types.ts` 运行时 `import { t } from "./i18n"` |

> **踩坑记录**：模块级 `const`（`SRC_META`、`levelOptions`、`TECH_ARCH`）在 setup 时只求值一次，**不会**随语言切换刷新。凡含文案的常量表，一律改成函数 / `computed`，或改为存 key、模板内 `t()`。

### 未迁移（有意保留）

| 项 | 原因 |
| --- | --- |
| `store.ts` `PREVIEW_TOOLS` / `LUA_SAMPLE` / `rootRegistryName()` | DEV mock 与需与后端 `auto_registry_name` 一致的派生名，见第 8 节 |
| `Sidebar.vue` 的 `蜂巢` | 品牌字标 |
| `ProjectEnvRow.vue` / `AboutPage.vue` 中被注释掉的代码块 | 非渲染内容 |
| `updater.ts` / 各处 `console.warn` | 日志 |

---

## 11. 验证与自检

```bash
cd app
npm run build        # vue-tsc --noEmit && vite build
```

自检清单：

- [ ] 语言包两份 key 完全一致（编译即校验）。
- [ ] 状态 / 阶段文案来自 `stage.*` / `task.*`，无新增本地映射表。
- [ ] 无字符串拼接式文案（技术标识符拼接除外，且只作参数传入）。
- [ ] 模板中无 `t` 变量遮蔽（循环变量用语义名，勿用 `t`）。
- [ ] **含文案的模块级常量表**已改为函数 / `computed`，或改为存 key、模板内 `t()`。
- [ ] 全局扫描无残留硬编码中文（仅允许第 8 节列出的例外）：

  ```bash
  # 列出仍含非注释 CJK 的源文件
  python - <<'PY'
  import re, glob
  CJK = re.compile('[\u4e00-\u9fff]')
  for f in sorted(glob.glob('src/**/*.vue', recursive=True) + glob.glob('src/**/*.ts', recursive=True)):
      n = 0
      for line in open(f, encoding='utf-8'):
          if line.strip().startswith(('//', '*', '/*', '<!--')):
              continue
          n += len(CJK.findall(re.sub(r'//.*', '', re.sub(r'<!--.*?-->', '', line))))
      if n:
          print(f'{n:5d}  {f}')
  PY
  ```

- [ ] 切换语言后：侧栏、顶栏标题、队列抽屉、弹窗、提示、**各页面正文与描述文案**、Naive UI 内置文案同步刷新。
