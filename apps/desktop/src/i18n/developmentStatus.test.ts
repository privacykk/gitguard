import { describe, expect, it } from "vitest";
import {
  isDevelopmentStatusCode,
  toStatusMessageKey,
} from "./developmentStatus";

describe("development status mapping", () => {
  it("maps a known code to its message key", () => {
    expect(toStatusMessageKey("week1_confirmation_unavailable")).toBe(
      "development.status.week1_confirmation_unavailable",
    );
  });

  it("maps unknown values to the fail-closed message key", () => {
    for (const value of [undefined, null, "", "other", 42]) {
      expect(toStatusMessageKey(value)).toBe("development.status.unknown");
    }
  });

  it("rejects values outside the known code list", () => {
    expect(isDevelopmentStatusCode("week1_confirmation_unavailable")).toBe(
      true,
    );
    expect(isDevelopmentStatusCode("week1_confirmation_available")).toBe(false);
  });
});
