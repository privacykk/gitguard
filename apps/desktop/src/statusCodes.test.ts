import { describe, expect, it } from "vitest";
import {
  AUDIT_CHANNEL_CODES,
  AUDIT_EVENT_CODES,
  AUDIT_REASON_CODES,
  AUDIT_RESULT_CODES,
  COMMAND_ERROR_CODES,
  HOOK_STATUS_CODES,
  commandErrorCode,
  isCommandErrorCode,
  isHookStatusCode,
  toAuditChannelMessageKey,
  toAuditEventMessageKey,
  toAuditReasonMessageKey,
  toAuditResultMessageKey,
  toAuditResultTone,
  toCommandErrorHintKey,
  toCommandErrorMessageKey,
  toHookStatusMessageKey,
} from "./statusCodes";

describe("hook status codes", () => {
  it("maps every known status to its own localized key", () => {
    const keys = HOOK_STATUS_CODES.map((code) => toHookStatusMessageKey(code));

    expect(new Set(keys).size).toBe(HOOK_STATUS_CODES.length);
    for (const key of keys) {
      expect(key).toMatch(/^hookStatus\./);
    }
  });

  it("falls back to the unknown key instead of inferring a healthy hook", () => {
    expect(toHookStatusMessageKey("SomethingElse")).toBe("hookStatus.unknown");
    expect(toHookStatusMessageKey(null)).toBe("hookStatus.unknown");
    expect(isHookStatusCode("healthy")).toBe(false);
    expect(isHookStatusCode("Healthy")).toBe(true);
  });
});

describe("command error codes", () => {
  it("maps every backend error code to a specific message", () => {
    for (const code of COMMAND_ERROR_CODES) {
      expect(isCommandErrorCode(code)).toBe(true);
      expect(toCommandErrorMessageKey(code)).toBe(`errors.${code}`);
    }
  });

  it("never reports an unknown failure as success", () => {
    expect(isCommandErrorCode("email_already_taken")).toBe(false);
    expect(toCommandErrorMessageKey("email_already_taken")).toBe(
      "errors.unknown",
    );
    expect(toCommandErrorMessageKey(undefined)).toBe("errors.unknown");
  });

  it("reads the stable code out of a thrown command error", () => {
    expect(commandErrorCode({ code: "not_a_repository" })).toBe(
      "not_a_repository",
    );
    expect(commandErrorCode(new Error("boom"))).toBe("unknown");
    expect(commandErrorCode(null)).toBe("unknown");
  });

  it("gives every backend error code its own next step", () => {
    for (const code of COMMAND_ERROR_CODES) {
      expect(toCommandErrorHintKey(code)).toBe(`errors.hints.${code}`);
    }
  });

  it("falls back to the generic hint for unknown error codes", () => {
    expect(toCommandErrorHintKey("email_already_taken")).toBe(
      "errors.hints.unknown",
    );
    expect(toCommandErrorHintKey(undefined)).toBe("errors.hints.unknown");
  });
});

describe("audit codes", () => {
  it("maps every known event, result, channel and reason to its own key", () => {
    const eventKeys = AUDIT_EVENT_CODES.map(toAuditEventMessageKey);
    expect(new Set(eventKeys).size).toBe(AUDIT_EVENT_CODES.length);
    expect(eventKeys.every((key) => key.startsWith("audit.events."))).toBe(
      true,
    );

    const resultKeys = AUDIT_RESULT_CODES.map(toAuditResultMessageKey);
    expect(new Set(resultKeys).size).toBe(AUDIT_RESULT_CODES.length);

    const channelKeys = AUDIT_CHANNEL_CODES.map(toAuditChannelMessageKey);
    expect(new Set(channelKeys).size).toBe(AUDIT_CHANNEL_CODES.length);

    const reasonKeys = AUDIT_REASON_CODES.map(toAuditReasonMessageKey);
    expect(new Set(reasonKeys).size).toBe(AUDIT_REASON_CODES.length);
    expect(reasonKeys.every((key) => key.startsWith("audit.reasons."))).toBe(
      true,
    );
  });

  it("never infers an allowed result from an unknown code", () => {
    expect(toAuditResultMessageKey("mirrored")).toBe("audit.results.unknown");
    expect(toAuditEventMessageKey("pushed")).toBe("audit.events.unknown");
    expect(toAuditChannelMessageKey("email")).toBe("audit.channels.unknown");
    expect(toAuditReasonMessageKey("weird code")).toBe("audit.reasons.unknown");

    expect(toAuditResultTone("allowed")).toBe("success");
    expect(toAuditResultTone("rejected")).toBe("danger");
    // pending 与未知结果都用警示色，绝不看起来像已放行。
    expect(toAuditResultTone("pending")).toBe("warning");
    expect(toAuditResultTone(null)).toBe("warning");
  });
});
