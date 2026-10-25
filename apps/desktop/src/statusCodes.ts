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
  "audit_unreadable",
  "diagnostics_write_failed",
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

/**
 * 把错误码转成排查建议 key。
 *
 * 建议只是操作提示，不改变任何状态；未知码同样给出通用建议，
 * 不能因为“不知道该建议什么”就把失败显示成成功。
 */
export function toCommandErrorHintKey(value: unknown): string {
  return isCommandErrorCode(value)
    ? `errors.hints.${value}`
    : "errors.hints.unknown";
}

/** `AuditEventKind` 的已知取值（与 Rust `gitguard-audit` 保持一致）。 */
export const AUDIT_EVENT_CODES = [
  "request_created",
  "confirmed",
  "cancelled",
  "timeout",
  "failure",
] as const;

export type AuditEventCode = (typeof AUDIT_EVENT_CODES)[number];

/** `AuditResult` 的已知取值。 */
export const AUDIT_RESULT_CODES = ["pending", "allowed", "rejected"] as const;

export type AuditResultCode = (typeof AUDIT_RESULT_CODES)[number];

/** `AuditChannel` 的已知取值。 */
export const AUDIT_CHANNEL_CODES = ["terminal", "desktop"] as const;

export type AuditChannelCode = (typeof AUDIT_CHANNEL_CODES)[number];

/** 审计原因码；与 `gitguard_audit::reason` 一一对应。 */
export const AUDIT_REASON_CODES = [
  "confirmation_required",
  "user_confirmed",
  "user_cancelled",
  "confirmation_timeout",
  "terminal_unavailable",
  "ipc_unavailable",
  "confirmation_failed",
  "request_mismatch",
  "refs_changed",
  "commit_list_unavailable",
  "invalid_input",
  "io_error",
  "policy_unavailable",
  "internal_error",
] as const;

export type AuditReasonCode = (typeof AUDIT_REASON_CODES)[number];

export function isAuditEventCode(value: unknown): value is AuditEventCode {
  return AUDIT_EVENT_CODES.some((code) => code === value);
}

export function isAuditResultCode(value: unknown): value is AuditResultCode {
  return AUDIT_RESULT_CODES.some((code) => code === value);
}

export function isAuditChannelCode(value: unknown): value is AuditChannelCode {
  return AUDIT_CHANNEL_CODES.some((code) => code === value);
}

/** 审计事件类型的文案 key；未知值落到通用的“未知事件”。 */
export function toAuditEventMessageKey(value: unknown): string {
  return isAuditEventCode(value)
    ? `audit.events.${value}`
    : "audit.events.unknown";
}

/** 审计结果的文案 key；未知值不推断成“已放行”。 */
export function toAuditResultMessageKey(value: unknown): string {
  return isAuditResultCode(value)
    ? `audit.results.${value}`
    : "audit.results.unknown";
}

/** 审计结果的色调；只有明确的 `allowed` 才用绿色。 */
export function toAuditResultTone(value: unknown): StatusTone {
  if (value === "allowed") {
    return "success";
  }
  if (value === "rejected") {
    return "danger";
  }
  return "warning";
}

/** 确认通道的文案 key；未知通道保留为未知，不假设为桌面端。 */
export function toAuditChannelMessageKey(value: unknown): string {
  return isAuditChannelCode(value)
    ? `audit.channels.${value}`
    : "audit.channels.unknown";
}

/** 原因码的文案 key；未知原因码只显示“未知原因”，不回显原始字符串。 */
export function toAuditReasonMessageKey(value: unknown): string {
  return AUDIT_REASON_CODES.some((code) => code === value)
    ? `audit.reasons.${String(value)}`
    : "audit.reasons.unknown";
}
