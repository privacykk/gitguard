import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App.vue";
import { DEFAULT_LOCALE, i18n, setLocale } from "./i18n";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => Promise.resolve("week1_confirmation_unavailable")),
}));

function mountApp() {
  return mount(App, {
    global: { plugins: [i18n] },
  });
}

describe("App", () => {
  // i18n 初始语言跟随系统（jsdom 为 en-US），测试前显式固定。
  beforeEach(() => {
    setLocale(DEFAULT_LOCALE);
  });

  afterEach(() => {
    setLocale(DEFAULT_LOCALE);
  });

  it("explains that confirmation fails closed during the prototype", async () => {
    const wrapper = mountApp();
    await flushPromises();

    expect(wrapper.get("h1").text()).toBe("GitGuard");
    expect(wrapper.text()).toContain("需要确认的推送会被拒绝");
  });

  it("renders localized copy after switching the interface language", async () => {
    const wrapper = mountApp();
    await flushPromises();

    await wrapper.get("#locale-select").setValue("en-US");

    expect(wrapper.get("#status-title").text()).toContain("Development status");
    expect(wrapper.text()).toContain(
      "pushes that need confirmation are rejected",
    );
    expect(document.documentElement.lang).toBe("en-US");
  });
});
