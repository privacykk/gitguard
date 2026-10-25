import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  GitGuardCommandError,
  buildDiagnostics,
  copyText,
  exportDiagnostics,
  getAuditLog,
  getPendingPush,
  getSettings,
  listenOpenSettings,
  parseAuditLog,
  parsePushConfirmation,
  pickRepositoryDirectory,
  setAppMenu,
} from "./api";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  openDialog: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.openDialog }));

/** 一份形状合法的待确认请求，用于验证「合法形状通过、缺字段拒绝」。 */
function validRequest() {
  return {
    requestId: "request-1",
    repositoryPath: "/tmp/example",
    remoteName: "origin",
    remoteUrl: "git@example.com:team/example.git",
    refs: [
      {
        localRef: "refs/heads/main",
        remoteRef: "refs/heads/main",
        kind: "create",
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
    receivedAtEpochSecs: 1_790_000_000,
  };
}

describe("parsePushConfirmation", () => {
  it("accepts a complete payload", () => {
    const parsed = parsePushConfirmation(validRequest());

    expect(parsed?.requestId).toBe("request-1");
    expect(parsed?.refs[0]?.kind).toBe("create");
    expect(parsed?.refs[0]?.commits[0]?.shortSha).toBe("0123456");
  });

  it("keeps the rewrite kind that flags force updates", () => {
    const request = validRequest();
    request.refs[0].kind = "rewrite";

    expect(parsePushConfirmation(request)?.refs[0]?.kind).toBe("rewrite");
  });

  it("rejects payloads that would let the UI confirm an unknown push", () => {
    const withoutRequestId = validRequest();
    delete (withoutRequestId as Record<string, unknown>).requestId;
    expect(parsePushConfirmation(withoutRequestId)).toBeNull();

    const unknownKind = validRequest();
    unknownKind.refs[0].kind = "mirror";
    expect(parsePushConfirmation(unknownKind)).toBeNull();

    const missingCommitField = validRequest();
    delete (missingCommitField.refs[0].commits[0] as Record<string, unknown>)
      .author;
    expect(parsePushConfirmation(missingCommitField)).toBeNull();

    expect(parsePushConfirmation(null)).toBeNull();
    expect(parsePushConfirmation("request")).toBeNull();
  });
});

describe("command wrappers", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
  });

  it("returns null instead of a guessed request when the payload is wrong", async () => {
    mocks.invoke.mockResolvedValue({ requestId: 42 });

    await expect(getPendingPush()).resolves.toBeNull();
  });

  it("rejects with a stable error code instead of rendering unchecked settings", async () => {
    mocks.invoke.mockResolvedValue({
      global: { requireConfirmationForUnregisteredRepositories: true },
    });

    await expect(getSettings()).rejects.toBeInstanceOf(GitGuardCommandError);
    await expect(getSettings()).rejects.toMatchObject({
      code: "internal_error",
    });
  });

  it("accepts settings that include the configuration path", async () => {
    mocks.invoke.mockResolvedValue({
      global: {
        requireConfirmationForUnregisteredRepositories: false,
        confirmationTimeoutSeconds: 180,
      },
      repositories: [],
      configError: null,
      ipcError: null,
      configPath: "/tmp/gitguard/config.json",
    });

    await expect(getSettings()).resolves.toMatchObject({
      configPath: "/tmp/gitguard/config.json",
    });
  });

  it("forwards the interface locale when rebuilding the native menu", async () => {
    mocks.invoke.mockResolvedValue(null);

    await setAppMenu("en-US");

    expect(mocks.invoke).toHaveBeenCalledWith("set_app_menu", {
      locale: "en-US",
    });
  });

  it("forwards the last error code when building the diagnostics summary", async () => {
    mocks.invoke.mockResolvedValue("gitguard diagnostics\n");

    await expect(buildDiagnostics("request_invalid")).resolves.toBe(
      "gitguard diagnostics\n",
    );
    expect(mocks.invoke).toHaveBeenCalledWith("build_diagnostics", {
      lastErrorCode: "request_invalid",
    });
  });

  it("returns the exported diagnostics path", async () => {
    mocks.invoke.mockResolvedValue("/tmp/gitguard/diagnostics-1.txt");

    await expect(exportDiagnostics(null)).resolves.toBe(
      "/tmp/gitguard/diagnostics-1.txt",
    );
    expect(mocks.invoke).toHaveBeenCalledWith("export_diagnostics", {
      lastErrorCode: null,
    });
  });

  it("rejects with a stable code when the audit payload is incomplete", async () => {
    mocks.invoke.mockResolvedValue({ path: "/tmp/gitguard/audit.jsonl" });

    await expect(getAuditLog()).rejects.toMatchObject({
      code: "internal_error",
    });
  });
});

describe("listenOpenSettings", () => {
  beforeEach(() => {
    mocks.listen.mockReset();
  });

  it("subscribes to the menu event and returns the unsubscribe function", async () => {
    const stop = vi.fn();
    mocks.listen.mockResolvedValue(stop);

    const result = await listenOpenSettings(() => undefined);

    expect(mocks.listen).toHaveBeenCalledWith(
      "open-settings",
      expect.any(Function),
    );
    result();
    expect(stop).toHaveBeenCalled();
  });
});

describe("pickRepositoryDirectory", () => {
  it("returns the chosen folder", async () => {
    mocks.openDialog.mockResolvedValue("/tmp/repository");

    await expect(pickRepositoryDirectory()).resolves.toBe("/tmp/repository");
    expect(mocks.openDialog).toHaveBeenCalledWith({
      directory: true,
      multiple: false,
    });
  });

  it("treats cancellation and unexpected results as no selection", async () => {
    mocks.openDialog.mockResolvedValue(null);
    await expect(pickRepositoryDirectory()).resolves.toBeNull();

    mocks.openDialog.mockResolvedValue(["/tmp/a", "/tmp/b"]);
    await expect(pickRepositoryDirectory()).resolves.toBeNull();
  });
});

/** 一份形状合法的审计页数据；字段名与 Rust 视图的 camelCase 一致。 */
function validAuditLog() {
  return {
    path: "/tmp/gitguard/audit.jsonl",
    retentionDays: 30,
    maxEntries: 2000,
    events: [
      {
        timestampEpochSecs: 1_790_000_000,
        timestamp: "2026-09-21T14:13:20Z",
        event: "confirmed",
        result: "allowed",
        reasonCode: "user_confirmed",
        channel: "desktop",
        repositoryPath: "/tmp/example",
        repositoryId: "repo-1",
        remoteName: "origin",
        remoteUrl: "https://***@example.com/team/example.git",
        requestId: "request-1",
        refCount: 1,
      },
    ],
    skippedLines: 0,
    truncated: false,
    loadError: null,
    configError: null,
    ipcError: null,
  };
}

describe("parseAuditLog", () => {
  it("accepts a complete payload and keeps already-redacted values", () => {
    const parsed = parseAuditLog(validAuditLog());

    expect(parsed?.events).toHaveLength(1);
    expect(parsed?.events[0]?.result).toBe("allowed");
    expect(parsed?.events[0]?.remoteUrl).toBe(
      "https://***@example.com/team/example.git",
    );
    expect(parsed?.retentionDays).toBe(30);
  });

  it("treats missing optional ids as absent instead of inventing them", () => {
    const payload = validAuditLog();
    delete (payload.events[0] as Record<string, unknown>).repositoryId;
    delete (payload.events[0] as Record<string, unknown>).requestId;

    const parsed = parseAuditLog(payload);

    expect(parsed?.events[0]?.repositoryId).toBeNull();
    expect(parsed?.events[0]?.requestId).toBeNull();
  });

  it("rejects unknown event, result or channel codes", () => {
    const unknownEvent = validAuditLog();
    unknownEvent.events[0].event = "pushed";
    expect(parseAuditLog(unknownEvent)).toBeNull();

    const unknownResult = validAuditLog();
    unknownResult.events[0].result = "mirrored";
    expect(parseAuditLog(unknownResult)).toBeNull();

    const unknownChannel = validAuditLog();
    unknownChannel.events[0].channel = "email";
    expect(parseAuditLog(unknownChannel)).toBeNull();

    expect(parseAuditLog(null)).toBeNull();
    expect(parseAuditLog({ path: "/tmp/audit.jsonl" })).toBeNull();
  });
});

describe("copyText", () => {
  afterEach(() => {
    Reflect.deleteProperty(window.navigator, "clipboard");
  });

  it("reports failure when the clipboard is unavailable", async () => {
    await expect(copyText("text")).resolves.toBe(false);
  });

  it("reports failure instead of throwing when the write is rejected", async () => {
    Object.defineProperty(window.navigator, "clipboard", {
      value: { writeText: vi.fn().mockRejectedValue(new Error("denied")) },
      configurable: true,
    });

    await expect(copyText("text")).resolves.toBe(false);
  });

  it("writes the summary when the clipboard allows it", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(window.navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });

    await expect(copyText("gitguard diagnostics")).resolves.toBe(true);
    expect(writeText).toHaveBeenCalledWith("gitguard diagnostics");
  });
});
