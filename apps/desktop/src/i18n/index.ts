import { createI18n } from "vue-i18n";
import enUS from "./messages/en-US";
import zhCN from "./messages/zh-CN";

export const SUPPORTED_LOCALES = ["zh-CN", "en-US"] as const;

export type SupportedLocale = (typeof SUPPORTED_LOCALES)[number];

export const DEFAULT_LOCALE: SupportedLocale = "zh-CN";

const LOCALE_STORAGE_KEY = "gitguard.locale";

/** 判断任意值是否是受支持的语言标识（用于收窄类型、校验存储值）。 */
export function isSupportedLocale(value: unknown): value is SupportedLocale {
  return SUPPORTED_LOCALES.some((locale) => locale === value);
}

/** 把 BCP 47 标签（如 `zh-Hant-TW`）归一到受支持的语言，未覆盖时返回 null。 */
export function matchLocale(tag: string): SupportedLocale | null {
  const normalized = tag.trim().toLowerCase();
  if (normalized.startsWith("zh")) {
    return "zh-CN";
  }
  if (normalized.startsWith("en")) {
    return "en-US";
  }
  return null;
}

function readStoredLocale(): string | null {
  try {
    return localStorage.getItem(LOCALE_STORAGE_KEY);
  } catch {
    return null;
  }
}

/**
 * 选出初始语言：本地记忆 → 系统语言 → 简体中文。
 * 参数带默认值，便于测试时传入固定值而不依赖浏览器环境。
 */
export function detectLocale(
  stored: string | null = readStoredLocale(),
  preferred: readonly string[] = navigator.languages ?? [navigator.language],
): SupportedLocale {
  if (isSupportedLocale(stored)) {
    return stored;
  }
  for (const tag of preferred) {
    const matched = matchLocale(tag);
    if (matched !== null) {
      return matched;
    }
  }
  return DEFAULT_LOCALE;
}

// legacy: false 表示使用 Composition API（组件里通过 useI18n() 取 t）。
export const i18n = createI18n({
  legacy: false,
  locale: detectLocale(),
  fallbackLocale: DEFAULT_LOCALE,
  messages: {
    "zh-CN": zhCN,
    "en-US": enUS,
  },
});

/** 切换语言：同时更新文案、`<html lang>` 与本地记忆，三者保持一致。 */
export function setLocale(locale: SupportedLocale): void {
  i18n.global.locale.value = locale;
  document.documentElement.lang = locale;
  try {
    localStorage.setItem(LOCALE_STORAGE_KEY, locale);
  } catch {
    // 存储不可用时只影响下次启动的语言，不影响本次渲染。
  }
}

// 首次加载时把检测结果同步到 `<html lang>`，供浏览器与辅助技术识别页面语言。
document.documentElement.lang = i18n.global.locale.value;
