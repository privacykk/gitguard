import { afterEach, describe, expect, it } from "vitest";
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

    expect(i18n.global.t("development.title")).toBe("Development status");
    expect(document.documentElement.lang).toBe("en-US");
    expect(localStorage.getItem("gitguard.locale")).toBe("en-US");
  });
});
