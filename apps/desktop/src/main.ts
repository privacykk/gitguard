import { createApp } from "vue";
import App from "./App.vue";
import { elementPlus } from "./elementPlus";
import { i18n } from "./i18n";

// Element Plus 的样式在 style.css 里以 `layer(element-plus)` 引入，
// 保证 Tailwind 原子类仍能覆盖组件默认样式。
import "./style.css";

// i18n 必须在 mount 前注册，否则组件里的 useI18n() 拿不到实例。
// Element Plus 按需注册，组件清单与样式清单都在 elementPlus.ts / style.css 里维护。
createApp(App).use(i18n).use(elementPlus).mount("#app");
