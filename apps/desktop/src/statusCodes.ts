/**
 * Rust 后端返回的稳定状态码 → i18n 文案 key 的映射。
 *
 * 后端只返回这些码，绝不返回句子；未知或形状不对的值一律落到 fail-closed 文案，
 * 绝不推断成「正常」或「推送已放行」。
 */

/** `HookStatus` 的已知取值（与 Rust `gitguard-config` 保持一致）。 */
export const HOOK_STATUS_CODES = [
  "Healthy",
  "Missing",
  "Modified",
  "Conflict",
  "Unknown",
] as const;

export type HookStatusCode = (typeof HOOK_STATUS_CODES)[number];

const HOOK_STATUS_MESSAGE_KEYS: Record<HookStatusCode, string> = {
  Healthy: "hookStatus.healthy",
  Missing: "hookStatus.missing",
  Modified: "hookStatus.modified",
  Conflict: "hookStatus.conflict",
  Unknown: "hookStatus.unknown",
};

/** 判断 command 返回值是否属于已知的 Hook 状态码。 */
export function isHookStatusCode(value: unknown): value is HookStatusCode {
  return HOOK_STATUS_CODES.some((code) => code === value);
}

/** 把 Hook 状态码转成文案 key；未知值落到 `unknown` 文案。 */
export function toHookStatusMessageKey(value: unknown): string {
  return isHookStatusCode(value)
    ? HOOK_STATUS_MESSAGE_KEYS[value]
    : HOOK_STATUS_MESSAGE_KEYS.Unknown;
}

/** 界面展示用的色调；只影响颜色，不参与任何放行判断。 */
export type StatusTone = "success" | "warning" | "danger" | "info";

const HOOK_STATUS_TONES: Record<HookStatusCode, StatusTone> = {
  Healthy: "success",
  Missing: "info",
  Modified: "danger",
  Conflict: "danger",
  // 无法判断一律用警示色，绝不能让它看起来和「正常」一样。
  Unknown: "warning",
};

/** 把 Hook 状态码转成色调；未知值同样归入警示色。 */
export function toHookStatusTone(value: unknown): StatusTone {
  return isHookStatusCode(value)
    ? HOOK_STATUS_TONES[value]
    : HOOK_STATUS_TONES.Unknown;
}

/** 后端命令可能返回的错误码（与 `apps/desktop/src-tauri/src/lib.rs` 保持一致）。 */
export const COMMAND_ERROR_CODES = [
  "configuration_unreadable",
  "configuration_write_failed",
  "repository_not_found",
  "hook_install_failed",
  "cli_binary_not_found",
  "not_a_repository",
  "path_not_absolute",
  "no_pending_request",
  "request_invalid",
  "invalid_timeout",
  "ipc_unavailable",
  "internal_error",
] as const;

export type CommandErrorCode = (typeof COMMAND_ERROR_CODES)[number];

const UNKNOWN_ERROR_MESSAGE_KEY = "errors.unknown";

/** 判断一个值是否属于已知的错误码。 */
export function isCommandErrorCode(value: unknown): value is CommandErrorCode {
  return COMMAND_ERROR_CODES.some((code) => code === value);
}

/** 从抛出的错误对象里取出稳定错误码；取不到时返回 `unknown`。 */
export function commandErrorCode(reason: unknown): string {
  if (typeof reason === "object" && reason !== null && "code" in reason) {
    const { code } = reason as { code: unknown };
    if (typeof code === "string") {
      return code;
    }
  }
  return "unknown";
}

/** 把错误码转成文案 key；未知码落到通用的 fail-closed 文案。 */
export function toCommandErrorMessageKey(value: unknown): string {
  return isCommandErrorCode(value)
    ? `errors.${value}`
    : UNKNOWN_ERROR_MESSAGE_KEY;
}
