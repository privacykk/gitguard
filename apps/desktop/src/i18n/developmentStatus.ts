/**
 * Rust `development_status` command 返回的稳定状态码。
 * 展示文案由前端按当前语言渲染，状态码本身不随语言变化。
 */
export const DEVELOPMENT_STATUS_CODES = [
  "week1_confirmation_unavailable",
] as const;

export type DevelopmentStatusCode = (typeof DEVELOPMENT_STATUS_CODES)[number];

const STATUS_MESSAGE_PREFIX = "development.status";

const UNKNOWN_STATUS_MESSAGE_KEY = `${STATUS_MESSAGE_PREFIX}.unknown`;

/** 判断 command 返回值是否属于已知状态码，用于类型收窄。 */
export function isDevelopmentStatusCode(
  value: unknown,
): value is DevelopmentStatusCode {
  return DEVELOPMENT_STATUS_CODES.some((code) => code === value);
}

/** 把 command 返回值收窄为已知文案 key；未知值一律落到 fail-closed 文案。 */
export function toStatusMessageKey(code: unknown): string {
  return isDevelopmentStatusCode(code)
    ? `${STATUS_MESSAGE_PREFIX}.${code}`
    : UNKNOWN_STATUS_MESSAGE_KEY;
}
