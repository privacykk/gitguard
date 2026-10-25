import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App.vue";
import { elementPlus } from "./elementPlus";
import { DEFAULT_LOCALE, i18n, setLocale } from "./i18n";

/** Element Plus 的部分组件会监听 ResizeObserver；jsdom 没有实现，补一个空壳。 */
class ResizeObserverStub {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

// 用 hoisted 保证 mock 工厂里引用的是同一批替身，而不是模块初始化后才创建的对象。
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  openDialog: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.openDialog }));

const REPOSITORY = {
  repositoryId: "repo-1",
  canonicalPath: "/tmp/example",
  pushConfirmationEnabled: true,
  hookStatus: "Healthy",
};

const SETTINGS = {
  global: {
    requireConfirmationForUnregisteredRepositories: true,
    confirmationTimeoutSeconds: 180,
  },
  repositories: [REPOSITORY],
  configError: null,
  ipcError: null,
  configPath: "/tmp/gitguard/config.json",
};

/** 每次调用都重新构造，避免测试之间共享可变对象。 */
function confirmationRequest() {
  return {
    requestId: "request-1",
    repositoryPath: "/tmp/example",
    remoteName: "origin",
    remoteUrl: "git@example.com:team/example.git",
    refs: [
      {
        localRef: "refs/heads/main",
        remoteRef: "refs/heads/main",
        kind: "update",
        commits: [
          {
            sha: "0123456789abcdef0123456789abcdef01234567",
            shortSha: "0123456",
            author: "Ada",
            timestamp: "2026-10-24 10:00:00 +0800",
            subject: "Add push confirmation",
          },
        ],
      },
    ],
    timeoutSeconds: 180,
    receivedAtEpochSecs: Math.floor(Date.now() / 1000),
  };
}

/** 后端命令的默认替身；单个测试按需覆盖其中一个命令。 */
function defaultInvoke(command: string): Promise<unknown> {
  switch (command) {
    case "get_pending_push_request":
      return Promise.resolve(null);
    case "get_settings":
      return Promise.resolve(SETTINGS);
    case "confirm_push":
      return Promise.resolve(null);
    case "cancel_push":
    case "set_app_menu":
    case "add_repository":
    case "toggle_repo_protection":
    case "set_unregistered_protection":
    case "set_confirmation_timeout":
      return Promise.resolve(null);
    default:
      return Promise.reject(new Error(`unexpected command: ${command}`));
  }
}

/** 让某个命令返回待确认请求，其余命令保持默认行为。 */
function pendingPushInvoke(command: string): Promise<unknown> {
  return command === "get_pending_push_request"
    ? Promise.resolve(confirmationRequest())
    : defaultInvoke(command);
}

/** 捕获后端推给前端的事件处理函数，便于模拟菜单与推送请求。 */
const eventHandlers = new Map<string, (event: { payload: unknown }) => void>();

let wrapper: VueWrapper | null = null;

function mountApp(): VueWrapper {
  // 用与 main.ts 相同的按需注册，避免测试里跑到实际不会注册的组件。
  wrapper = mount(App, { global: { plugins: [i18n, elementPlus] } });
  return wrapper;
}

/** 通过侧边栏按钮切换页面；按钮带 data-view 便于定位，不依赖文案。 */
async function openView(app: VueWrapper, view: string): Promise<void> {
  await app.get(`[data-view="${view}"]`).trigger("click");
  await flushPromises();
}

describe("App", () => {
  beforeEach(() => {
    // i18n 初始语言跟随系统（jsdom 为 en-US），测试前显式固定。
    setLocale(DEFAULT_LOCALE);
    // isDesktopAvailable() 通过该属性判断是否运行在 Tauri 容器内。
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
      writable: true,
    });
    mocks.invoke.mockReset();
    mocks.invoke.mockImplementation(defaultInvoke);
    eventHandlers.clear();
    mocks.listen.mockReset();
    mocks.listen.mockImplementation(
      (event: string, handler: (payload: { payload: unknown }) => void) => {
        eventHandlers.set(event, handler);
        return Promise.resolve(() => undefined);
      },
    );
    mocks.openDialog.mockReset();
    mocks.openDialog.mockResolvedValue(null);
    vi.stubGlobal("ResizeObserver", ResizeObserverStub);
    // 侧边栏收起状态会写入本地记忆，用例之间不能互相影响。
    localStorage.removeItem("gitguard.sidebarCollapsed");
  });

  afterEach(() => {
    wrapper?.unmount();
    wrapper = null;
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    vi.unstubAllGlobals();
    setLocale(DEFAULT_LOCALE);
  });

  it("opens on the dashboard with localized navigation and real counts", async () => {
    const app = mountApp();
    await flushPromises();

    expect(app.get("#dashboard-title").text()).toBe(
      i18n.global.t("dashboard.title"),
    );
    expect(app.text()).toContain(i18n.global.t("nav.repositories"));
    expect(app.text()).toContain(i18n.global.t("nav.settings"));
    expect(app.text()).toContain(i18n.global.t("dashboard.statusRunning"));
    expect(app.get('[data-view="dashboard"]').attributes("aria-current")).toBe(
      "page",
    );

    const stats = app.findAll("[data-stat]");
    expect(stats).toHaveLength(4);
    expect(stats[0].text()).toContain("1");
  });

  it("lists protected repositories with their hook status", async () => {
    const app = mountApp();
    await flushPromises();
    await openView(app, "repositories");

    expect(app.get("#repositories-title").text()).toBe(
      i18n.global.t("repositories.title"),
    );
    expect(app.text()).toContain(REPOSITORY.canonicalPath);
    expect(app.text()).toContain(i18n.global.t("hookStatus.healthy"));
  });

  it("treats unknown hook status codes as fail-closed", async () => {
    mocks.invoke.mockImplementation((command: string) =>
      command === "get_settings"
        ? Promise.resolve({
            ...SETTINGS,
            repositories: [{ ...REPOSITORY, hookStatus: "Unrecognized" }],
          })
        : defaultInvoke(command),
    );

    const app = mountApp();
    await flushPromises();
    await openView(app, "repositories");

    expect(app.text()).toContain(i18n.global.t("hookStatus.unknown"));
  });

  it("marks identity and audit pages as planned instead of pretending they work", async () => {
    const app = mountApp();
    await flushPromises();

    await openView(app, "identities");
    expect(app.get("#identities-title").text()).toBe(
      i18n.global.t("identities.title"),
    );
    expect(app.text()).toContain(i18n.global.t("identities.unavailableTitle"));

    await openView(app, "audit");
    expect(app.get("#audit-title").text()).toBe(i18n.global.t("audit.title"));
    expect(app.text()).toContain(i18n.global.t("audit.unavailableTitle"));
  });

  it("stays read-only when the desktop backend is unavailable", async () => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");

    const app = mountApp();
    await flushPromises();

    expect(app.text()).toContain(i18n.global.t("app.desktopUnavailable"));
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("jumps to the approval page when a push request is pending", async () => {
    mocks.invoke.mockImplementation(pendingPushInvoke);

    const app = mountApp();
    await flushPromises();

    expect(app.get("#approval-title").text()).toBe(
      i18n.global.t("approval.title"),
    );
  });

  it("requires the user to type yes before confirming a push", async () => {
    mocks.invoke.mockImplementation(pendingPushInvoke);

    const app = mountApp();
    await flushPromises();

    expect(app.get("#push-confirmation-title").text()).toBe(
      i18n.global.t("push.title"),
    );
    expect(app.text()).toContain("Add push confirmation");

    const confirm = app.get("[data-confirm]");
    expect(confirm.attributes("disabled")).toBeDefined();

    await app.get("#confirmation-answer").setValue("y");
    expect(confirm.attributes("disabled")).toBeDefined();

    await app.get("#confirmation-answer").setValue("yes");
    expect(confirm.attributes("disabled")).toBeUndefined();

    await app.get("form[data-confirm-form]").trigger("submit");
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("confirm_push", {
      requestId: "request-1",
    });
    expect(app.text()).toContain(i18n.global.t("push.confirmed"));
  });

  it("clears the typed answer when a different push request arrives", async () => {
    // 回归：确认面板曾复用同一个组件实例，新请求会沿用上一次输入的 yes，
    // 让「必须手输 yes」的约束被绕过。
    let current = confirmationRequest();
    mocks.invoke.mockImplementation((command: string) =>
      command === "get_pending_push_request"
        ? Promise.resolve(current)
        : defaultInvoke(command),
    );

    const app = mountApp();
    await flushPromises();

    await app.get("#confirmation-answer").setValue("yes");
    expect(app.get("[data-confirm]").attributes("disabled")).toBeUndefined();

    current = { ...confirmationRequest(), requestId: "request-2" };
    eventHandlers.get("push-request-received")?.({ payload: current });
    await flushPromises();

    const input = app.get("#confirmation-answer").element as HTMLInputElement;
    expect(input.value).toBe("");
    expect(app.get("[data-confirm]").attributes("disabled")).toBeDefined();
  });

  it("shows a fail-closed message and no success notice when confirming fails", async () => {
    mocks.invoke.mockImplementation((command: string) =>
      command === "get_pending_push_request"
        ? Promise.resolve(confirmationRequest())
        : command === "confirm_push"
          ? Promise.reject({ code: "request_invalid" })
          : defaultInvoke(command),
    );

    const app = mountApp();
    await flushPromises();

    await app.get("#confirmation-answer").setValue("yes");
    await app.get("form[data-confirm-form]").trigger("submit");
    await flushPromises();

    expect(app.text()).toContain(i18n.global.t("errors.request_invalid"));
    expect(app.text()).not.toContain(i18n.global.t("push.confirmed"));
  });

  it("cancels the pending push without executing it", async () => {
    mocks.invoke.mockImplementation(pendingPushInvoke);

    const app = mountApp();
    await flushPromises();

    await app.get("[data-cancel]").trigger("click");
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("cancel_push", {
      requestId: "request-1",
    });
    expect(mocks.invoke).not.toHaveBeenCalledWith(
      "confirm_push",
      expect.anything(),
    );
  });

  it("keeps a banner while a pending push is waiting on another page", async () => {
    mocks.invoke.mockImplementation(pendingPushInvoke);

    const app = mountApp();
    await flushPromises();
    await openView(app, "repositories");

    const banner = app.get("[data-pending-banner]");
    expect(banner.text()).toContain(i18n.global.t("approval.pendingTitle"));

    await banner.get("button").trigger("click");
    await flushPromises();

    expect(app.get("#approval-title").text()).toBe(
      i18n.global.t("approval.title"),
    );
  });

  it("fills the repository path from the folder picker", async () => {
    mocks.openDialog.mockResolvedValue("/tmp/picked");

    const app = mountApp();
    await flushPromises();
    await openView(app, "repositories");

    await app.get("[data-browse]").trigger("click");
    await flushPromises();

    expect(mocks.openDialog).toHaveBeenCalledWith({
      directory: true,
      multiple: false,
    });
    expect(
      (app.get("#repository-path").element as HTMLInputElement).value,
    ).toBe("/tmp/picked");
  });

  it("cancelling the folder picker keeps the current path", async () => {
    const app = mountApp();
    await flushPromises();
    await openView(app, "repositories");

    await app.get("#repository-path").setValue("/tmp/typed");
    await app.get("[data-browse]").trigger("click");
    await flushPromises();

    expect(
      (app.get("#repository-path").element as HTMLInputElement).value,
    ).toBe("/tmp/typed");
  });

  it("adds a repository with the protection choice from the form", async () => {
    const app = mountApp();
    await flushPromises();
    await openView(app, "repositories");

    await app.get("#repository-path").setValue("/tmp/example");
    await app.get("[data-add-protection]").trigger("click");
    await app.get("form[data-add-form]").trigger("submit");
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("add_repository", {
      path: "/tmp/example",
      enabled: true,
    });
  });

  it("does not inherit the unregistered setting when adding a repository", async () => {
    // 回归：添加表单曾跟随全局开关，于是全局的开/关会静静改变「添加后是否受保护」。
    // 现在两者独立：全局为 true 时，添加表单仍然默认不勾选。
    const app = mountApp();
    await flushPromises();
    await openView(app, "repositories");

    const protection = app.get("[data-add-protection]");
    expect(protection.get("input").attributes("aria-checked")).toBe("false");

    await app.get("#repository-path").setValue("/tmp/example");
    await app.get("form[data-add-form]").trigger("submit");
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("add_repository", {
      path: "/tmp/example",
      enabled: false,
    });
  });

  it("toggles push protection from the repository switch", async () => {
    const app = mountApp();
    await flushPromises();
    await openView(app, "repositories");

    const toggle = app.get("[data-repo-switch]");
    expect(toggle.get("input").attributes("aria-checked")).toBe("true");

    await toggle.trigger("click");
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("toggle_repo_protection", {
      repositoryId: "repo-1",
      enabled: false,
    });
  });

  it("changes the setting for unregistered repositories", async () => {
    const app = mountApp();
    await flushPromises();
    await openView(app, "policies");

    const toggle = app.get("[data-unregistered-switch]");
    expect(toggle.get("input").attributes("aria-checked")).toBe("true");

    await toggle.trigger("click");
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("set_unregistered_protection", {
      enabled: false,
    });
  });

  it("shows a stable fail-closed message when a command fails", async () => {
    mocks.invoke.mockImplementation((command: string) =>
      command === "add_repository"
        ? Promise.reject({ code: "not_a_repository" })
        : defaultInvoke(command),
    );

    const app = mountApp();
    await flushPromises();
    await openView(app, "repositories");

    await app.get("#repository-path").setValue("/tmp/missing");
    await app.get("form[data-add-form]").trigger("submit");
    await flushPromises();

    expect(app.text()).toContain(i18n.global.t("errors.not_a_repository"));
  });

  it("saves the confirmation timeout as a number", async () => {
    const app = mountApp();
    await flushPromises();
    await openView(app, "settings");

    await app.get("#confirmation-timeout").setValue("60");
    await app.get("form[data-timeout-form]").trigger("submit");
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("set_confirmation_timeout", {
      seconds: 60,
    });
  });

  it("shows the configuration file path on the settings page", async () => {
    const app = mountApp();
    await flushPromises();
    await openView(app, "settings");

    expect(app.text()).toContain(SETTINGS.configPath);
  });

  it("opens settings with the platform shortcut and closes it with Escape", async () => {
    const app = mountApp();
    await flushPromises();

    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: ",", metaKey: true }),
    );
    await flushPromises();

    expect(app.get("#settings-title").text()).toBe(
      i18n.global.t("settings.title"),
    );

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    await flushPromises();

    expect(app.get("#dashboard-title").text()).toBe(
      i18n.global.t("dashboard.title"),
    );
  });

  it("opens settings from the native menu event", async () => {
    const app = mountApp();
    await flushPromises();

    eventHandlers.get("open-settings")?.({ payload: null });
    await flushPromises();

    expect(app.get("#settings-title").text()).toBe(
      i18n.global.t("settings.title"),
    );
  });

  it("rebuilds the native menu when the interface language changes", async () => {
    const app = mountApp();
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("set_app_menu", {
      locale: "zh-CN",
    });

    await openView(app, "settings");
    await app.get("#locale-select").setValue("en-US");
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("set_app_menu", {
      locale: "en-US",
    });
    expect(document.documentElement.lang).toBe("en-US");
    expect(app.text()).toContain("Overview");
  });

  it("collapses the sidebar into icon-only navigation and remembers it", async () => {
    const app = mountApp();
    await flushPromises();

    expect(app.get("[data-sidebar-toggle]").attributes("aria-expanded")).toBe(
      "true",
    );
    expect(app.text()).toContain(i18n.global.t("nav.identities"));

    await app.get("[data-sidebar-toggle]").trigger("click");

    expect(app.get("[data-sidebar-toggle]").attributes("aria-expanded")).toBe(
      "false",
    );
    // 收起后不再显示文案，靠 aria-label 保留可读名称。
    expect(app.text()).not.toContain(i18n.global.t("nav.identities"));
    expect(app.get('[data-view="identities"]').attributes("aria-label")).toBe(
      i18n.global.t("nav.identities"),
    );
    expect(localStorage.getItem("gitguard.sidebarCollapsed")).toBe("true");
  });

  it("restores the collapsed sidebar from local storage", async () => {
    localStorage.setItem("gitguard.sidebarCollapsed", "true");
    const app = mountApp();
    await flushPromises();

    expect(app.get("[data-sidebar-toggle]").attributes("aria-expanded")).toBe(
      "false",
    );
    expect(app.get('[data-view="dashboard"]').text()).toBe("");
    // 页面切换在收起态下仍然可用。
    await openView(app, "policies");
    expect(app.get("#policies-title").text()).toBe(
      i18n.global.t("policies.title"),
    );
  });

  it("shows unknown instead of healthy when the settings cannot be loaded", async () => {
    // 回归：设置读不出来时，总览不能同时声称「配置文件正常」「IPC 运行中」。
    mocks.invoke.mockImplementation((command: string) =>
      command === "get_settings"
        ? Promise.reject(new Error("settings unavailable"))
        : defaultInvoke(command),
    );

    const app = mountApp();
    await flushPromises();

    const dashboard = app.get("#dashboard-title").element.closest("section");
    expect(dashboard?.textContent).toContain(
      i18n.global.t("dashboard.hooksUnknown"),
    );
    expect(dashboard?.textContent).toContain(i18n.global.t("common.unknown"));
    expect(dashboard?.textContent).not.toContain(i18n.global.t("common.ok"));
    // 未知也属于「有待处理」，不能被当成「尚未启用保护」。
    expect(dashboard?.textContent).toContain(
      i18n.global.t("dashboard.statusAttention"),
    );
  });

  it("keeps the policies switch locked while the settings are unavailable", async () => {
    mocks.invoke.mockImplementation((command: string) =>
      command === "get_settings"
        ? Promise.reject(new Error("settings unavailable"))
        : defaultInvoke(command),
    );

    const app = mountApp();
    await flushPromises();
    await openView(app, "policies");

    expect(app.get("[data-unregistered-switch]").classes()).toContain(
      "is-disabled",
    );
    await app.get("[data-unregistered-switch]").trigger("click");
    await flushPromises();
    expect(mocks.invoke).not.toHaveBeenCalledWith(
      "set_unregistered_protection",
      expect.anything(),
    );
  });

  it("refuses to add a repository while the settings are unavailable", async () => {
    mocks.invoke.mockImplementation((command: string) =>
      command === "get_settings"
        ? Promise.reject(new Error("settings unavailable"))
        : defaultInvoke(command),
    );

    const app = mountApp();
    await flushPromises();
    await openView(app, "repositories");

    await app.get("#repository-path").setValue("/tmp/example");
    await app.get("form[data-add-form]").trigger("submit");
    await flushPromises();

    expect(mocks.invoke).not.toHaveBeenCalledWith(
      "add_repository",
      expect.anything(),
    );
  });
});
