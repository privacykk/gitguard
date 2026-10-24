import { createApp } from "vue";
import App from "./App.vue";
import { i18n } from "./i18n";
import "./style.css";

// i18n 必须在 mount 前注册，否则组件里的 useI18n() 拿不到实例。
createApp(App).use(i18n).mount("#app");
