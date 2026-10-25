import { afterEach, describe, expect, it } from "vitest";
import {
  AUDIT_CHANNEL_CODES,
  AUDIT_EVENT_CODES,
  AUDIT_REASON_CODES,
  AUDIT_RESULT_CODES,
  COMMAND_ERROR_CODES,
  toAuditChannelMessageKey,
  toAuditEventMessageKey,
  toAuditReasonMessageKey,
  toAuditResultMessageKey,
  toCommandErrorHintKey,
  toCommandErrorMessageKey,
} from "../statusCodes";
import {
  DEFAULT_LOCALE,
  detectLocale,
  i18n,
  isSupportedLocale,
  matchLocale,
  setLocale,
} from "./index";

describe("locale detection", () => {
  it("prefers a supported stored locale", () => {
    expect(detectLocale("en-US", ["zh-CN"])).toBe("en-US");
  });

  it("ignores unsupported stored values and matches system languages", () => {
    expect(detectLocale("fr-FR", ["fr-FR", "en-GB"])).toBe("en-US");
    expect(detectLocale(null, ["zh-Hant-TW"])).toBe("zh-CN");
  });

  it("falls back to the default locale when nothing matches", () => {
    expect(detectLocale(null, ["fr-FR", "de-DE"])).toBe(DEFAULT_LOCALE);
    expect(matchLocale("ja-JP")).toBeNull();
  });

  it("only accepts exact supported identifiers", () => {
    expect(isSupportedLocale("zh-CN")).toBe(true);
    expect(isSupportedLocale("zh-cn")).toBe(false);
    expect(isSupportedLocale(undefined)).toBe(false);
  });
});

describe("locale switching", () => {
  afterEach(() => {
    setLocale(DEFAULT_LOCALE);
  });

  it("switches messages, document language and stored preference", () => {
    setLocale("en-US");

    expect(i18n.global.t("settings.title")).toBe("Settings");
    expect(i18n.global.t("nav.dashboard")).toBe("Overview");
    expect(document.documentElement.lang).toBe("en-US");
    expect(localStorage.getItem("gitguard.locale")).toBe("en-US");
  });
});

describe("message coverage", () => {
  afterEach(() => {
    setLocale(DEFAULT_LOCALE);
  });

  it("has a message and a hint for every stable code in both locales", () => {
    const keys = [
      ...COMMAND_ERROR_CODES.flatMap((code) => [
        toCommandErrorMessageKey(code),
        toCommandErrorHintKey(code),
      ]),
      ...AUDIT_EVENT_CODES.map(toAuditEventMessageKey),
      ...AUDIT_RESULT_CODES.map(toAuditResultMessageKey),
      ...AUDIT_CHANNEL_CODES.map(toAuditChannelMessageKey),
      ...AUDIT_REASON_CODES.map(toAuditReasonMessageKey),
    ];

    for (const locale of ["zh-CN", "en-US"] as const) {
      setLocale(locale);
      for (const key of keys) {
        // 缺少翻译时 vue-i18n 会回退成 key 本身，这里直接拦住。
        expect(i18n.global.t(key)).not.toBe(key);
      }
    }
  });
});
