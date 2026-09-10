// main.ts —— Vue 3 入口
import { createApp, nextTick } from "vue";
import App from "./App.vue";
import { i18n } from "./i18n";
import "./styles.css";

const app = createApp(App);
app.use(i18n);
app.mount("#root");

// 淡出并移除 splash 启动屏：
// · 等待 nextTick 确保 Vue 已把首个组件挂载到 DOM；
// · 加 .splash-fade-out 触发 CSS 过渡，结束后移除节点避免占用层叠上下文。
void nextTick(() => {
  const splash = document.getElementById("splash");
  if (!splash) return;
  splash.classList.add("splash-fade-out");
  const remove = () => splash.remove();
  splash.addEventListener("transitionend", remove, { once: true });
  // 兜底：过渡未触发（极端环境）也至少 600ms 后强制移除
  window.setTimeout(remove, 600);
});
