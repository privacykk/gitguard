/**
 * 桌面端的页面集合。
 *
 * 设置与六个业务页面分开：侧边栏把它放在底部，快捷键也直接落到这里，
 * 因此类型上单独列一个 `settings`，避免业务导航里出现重复入口。
 */
export const NAV_VIEWS = [
  "dashboard",
  "identities",
  "repositories",
  "policies",
  "approval",
  "audit",
] as const;

export type NavView = (typeof NAV_VIEWS)[number];

export type AppView = NavView | "settings";

/** 导航项到文案 key 的映射；新增页面时编译器会提醒补齐。 */
export const NAV_LABEL_KEYS: Record<NavView, string> = {
  dashboard: "nav.dashboard",
  identities: "nav.identities",
  repositories: "nav.repositories",
  policies: "nav.policies",
  approval: "nav.approval",
  audit: "nav.audit",
};
