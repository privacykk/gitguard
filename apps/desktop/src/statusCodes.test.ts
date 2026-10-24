import { describe, expect, it } from "vitest";
import {
  COMMAND_ERROR_CODES,
  HOOK_STATUS_CODES,
  commandErrorCode,
  isCommandErrorCode,
  isHookStatusCode,
  toCommandErrorMessageKey,
  toHookStatusMessageKey,
} from "./statusCodes";

describe("hook status codes", () => {
  it("maps every known status to its own localized key", () => {
    const keys = HOOK_STATUS_CODES.map((code) => toHookStatusMessageKey(code));

    expect(new Set(keys).size).toBe(HOOK_STATUS_CODES.length);
    for (const key of keys) {
      expect(key).toMatch(/^settings\.hookStatus\./);
    }
  });

  it("falls back to the unknown key instead of inferring a healthy hook", () => {
    expect(toHookStatusMessageKey("SomethingElse")).toBe(
      "settings.hookStatus.unknown",
    );
    expect(toHookStatusMessageKey(null)).toBe("settings.hookStatus.unknown");
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
});
