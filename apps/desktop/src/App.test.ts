import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App.vue";
import { DEFAULT_LOCALE, i18n, setLocale } from "./i18n";

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
  global: { defaultPushConfirmation: true, confirmationTimeoutSeconds: 180 },
  repositories: [REPOSITORY],
  configError: null,
  ipcError: null,
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
    case "add_repository":
    case "toggle_repo_protection":
    case "set_default_protection":
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

let wrapper: VueWrapper | null = null;

function mountApp(): VueWrapper {
  wrapper = mount(App, { global: { plugins: [i18n] } });
  return wrapper;
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
    mocks.listen.mockReset();
    mocks.listen.mockResolvedValue(() => undefined);
    mocks.openDialog.mockReset();
    mocks.openDialog.mockResolvedValue(null);
  });

  afterEach(() => {
    wrapper?.unmount();
    wrapper = null;
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    setLocale(DEFAULT_LOCALE);
  });

  it("lists protected repositories with their hook status", async () => {
    const app = mountApp();
    await flushPromises();

    expect(app.get("#settings-title").text()).toBe(
      i18n.global.t("settings.title"),
    );
    expect(app.text()).toContain(REPOSITORY.canonicalPath);
    expect(app.text()).toContain(i18n.global.t("settings.hookStatus.healthy"));
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

    expect(app.text()).toContain(i18n.global.t("settings.hookStatus.unknown"));
  });

  it("stays read-only when the desktop backend is unavailable", async () => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");

    const app = mountApp();
    await flushPromises();

    expect(app.text()).toContain(i18n.global.t("app.desktopUnavailable"));
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("requires the user to type yes before confirming a push", async () => {
    mocks.invoke.mockImplementation(pendingPushInvoke);

    const app = mountApp();
    await flushPromises();

    expect(app.get("#push-confirmation-title").text()).toBe(
      i18n.global.t("push.title"),
    );
    expect(app.text()).toContain("Add push confirmation");

    const confirm = app.get(".confirmation__confirm");
    expect(confirm.attributes("disabled")).toBeDefined();

    await app.get("#confirmation-answer").setValue("y");
    expect(confirm.attributes("disabled")).toBeDefined();

    await app.get("#confirmation-answer").setValue("yes");
    expect(confirm.attributes("disabled")).toBeUndefined();

    await app.get("form.confirmation__actions").trigger("submit");
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("confirm_push", {
      requestId: "request-1",
    });
    expect(app.text()).toContain(i18n.global.t("push.confirmed"));
  });

  it("clears the typed answer when a different push request arrives", async () => {
    // 回归：确认面板曾复用同一个组件实例，新请求会沿用上一次输入的 yes，
    // 让「必须手输 yes」的约束被绕过。
    let onEvent: ((event: { payload: unknown }) => void) | undefined;
    mocks.listen.mockImplementation(
      (
        _event: string,
        handler: (event: { payload: unknown }) => void,
      ): Promise<() => void> => {
        onEvent = handler;
        return Promise.resolve(() => undefined);
      },
    );

    let current = confirmationRequest();
    mocks.invoke.mockImplementation((command: string) =>
      command === "get_pending_push_request"
        ? Promise.resolve(current)
        : defaultInvoke(command),
    );

    const app = mountApp();
    await flushPromises();

    await app.get("#confirmation-answer").setValue("yes");
    expect(
      app.get(".confirmation__confirm").attributes("disabled"),
    ).toBeUndefined();

    current = { ...confirmationRequest(), requestId: "request-2" };
    onEvent?.({ payload: current });
    await flushPromises();

    const input = app.get("#confirmation-answer").element as HTMLInputElement;
    expect(input.value).toBe("");
    expect(
      app.get(".confirmation__confirm").attributes("disabled"),
    ).toBeDefined();
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
    await app.get("form.confirmation__actions").trigger("submit");
    await flushPromises();

    expect(app.text()).toContain(i18n.global.t("errors.request_invalid"));
    expect(app.text()).not.toContain(i18n.global.t("push.confirmed"));
  });

  it("cancels the pending push without executing it", async () => {
    mocks.invoke.mockImplementation(pendingPushInvoke);

    const app = mountApp();
    await flushPromises();

    await app.get(".confirmation__cancel").trigger("click");
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("cancel_push", {
      requestId: "request-1",
    });
    expect(mocks.invoke).not.toHaveBeenCalledWith(
      "confirm_push",
      expect.anything(),
    );
  });

  it("fills the repository path from the folder picker", async () => {
    mocks.openDialog.mockResolvedValue("/tmp/picked");

    const app = mountApp();
    await flushPromises();

    await app.get(".settings__button--secondary").trigger("click");
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

    await app.get("#repository-path").setValue("/tmp/typed");
    await app.get(".settings__button--secondary").trigger("click");
    await flushPromises();

    expect(
      (app.get("#repository-path").element as HTMLInputElement).value,
    ).toBe("/tmp/typed");
  });

  it("adds a repository with the protection choice from the form", async () => {
    const app = mountApp();
    await flushPromises();

    await app.get("#repository-path").setValue("/tmp/example");
    await app.get("form.settings__form").trigger("submit");
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("add_repository", {
      path: "/tmp/example",
      enabled: true,
    });
  });

  it("can add a repository with protection disabled", async () => {
    const app = mountApp();
    await flushPromises();

    await app.get("#repository-path").setValue("/tmp/example");
    await app.get('input[type="checkbox"]').setValue(false);
    await app.get("form.settings__form").trigger("submit");
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("add_repository", {
      path: "/tmp/example",
      enabled: false,
    });
  });

  it("toggles push protection from the repository switch", async () => {
    const app = mountApp();
    await flushPromises();

    const toggle = app.get(".settings__item .settings__switch");
    expect(toggle.attributes("aria-checked")).toBe("true");

    await toggle.trigger("click");
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("toggle_repo_protection", {
      repositoryId: "repo-1",
      enabled: false,
    });
  });

  it("changes the default protection for new repositories", async () => {
    const app = mountApp();
    await flushPromises();

    const toggle = app.get(".settings__default .settings__switch");
    expect(toggle.attributes("aria-checked")).toBe("true");

    await toggle.trigger("click");
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("set_default_protection", {
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

    await app.get("#repository-path").setValue("/tmp/missing");
    await app.get("form.settings__form").trigger("submit");
    await flushPromises();

    expect(app.text()).toContain(i18n.global.t("errors.not_a_repository"));
  });

  it("saves the confirmation timeout as a number", async () => {
    const app = mountApp();
    await flushPromises();

    await app.get("#confirmation-timeout").setValue("60");
    await app.findAll("form.settings__form")[1].trigger("submit");
    await flushPromises();

    expect(mocks.invoke).toHaveBeenCalledWith("set_confirmation_timeout", {
      seconds: 60,
    });
  });

  it("renders localized copy after switching the interface language", async () => {
    const app = mountApp();
    await flushPromises();

    await app.get("#locale-select").setValue("en-US");

    expect(app.get("#settings-title").text()).toBe("Repositories");
    expect(document.documentElement.lang).toBe("en-US");
  });
});
