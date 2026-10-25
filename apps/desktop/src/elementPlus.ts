import type { App, Plugin } from "vue";
import {
  ElAlert,
  ElBadge,
  ElButton,
  ElDescriptions,
  ElDescriptionsItem,
  ElIcon,
  ElInput,
  ElMenu,
  ElMenuItem,
  ElProgress,
  ElSkeleton,
  ElSwitch,
  ElTag,
  ElTooltip,
} from "element-plus";

/**
 * Element Plus 按需注册。
 *
 * 全量 `app.use(ElementPlus)` 会把整套组件打进产物，桌面端虽然不走网络分发，
 * 但启动解析与内存占用仍是实打实的成本。这里只注册界面实际用到的组件：
 * 新增模板里的 `el-*` 标签时必须同步这个列表（开发期 Vue 会打印未知组件警告），
 * 并在 `style.css` 里补上对应的 `el-*.css`。
 *
 * 类型检查不依赖运行时注册：`vite-env.d.ts` 引用了 `element-plus/global`。
 */
const COMPONENTS = [
  ElAlert,
  ElBadge,
  ElButton,
  ElDescriptions,
  ElDescriptionsItem,
  ElIcon,
  ElInput,
  ElMenu,
  ElMenuItem,
  ElProgress,
  ElSkeleton,
  ElSwitch,
  ElTag,
  ElTooltip,
];

/** 供 `main.ts` 与测试共用，保证两侧注册的组件集合一致。 */
export const elementPlus: Plugin = {
  install(app: App): void {
    for (const component of COMPONENTS) {
      // Element Plus 的组件都带 install（withInstall），用它注册能拿回自身名称，
      // 避免手写 `app.component(component.name, component)` 时处理可空的名字。
      app.use(component);
    }
  },
};
