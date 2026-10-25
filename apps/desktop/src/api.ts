import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  isAuditChannelCode,
  isAuditEventCode,
  isAuditResultCode,
  type AuditChannelCode,
  type AuditEventCode,
  type AuditResultCode,
} from "./statusCodes";

/**
 * 与 Rust 后端之间的数据契约。
 *
 * 后端返回的都是稳定状态码与纯数据，前端负责把状态码翻译成当前语言的文案；
 * 这里对 command 的返回值做形状校验：形状不对时按「没有待确认请求」处理，
 * 绝不猜测出一个可确认的请求。
 */

/** 单条提交的展示数据。 */
export interface CommitInfo {
  sha: string;
  shortSha: string;
  author: string;
  timestamp: string;
  subject: string;
}

/**
 * ref 更新的类型，决定界面上的风险提示。
 *
 * `rewrite` 表示本次更新会丢弃远端已有的提交（`--force` / `--force-with-lease`
 * 或 refspec 里的 `+`），只用于展示风险，不参与任何放行判断。
 */
export type RefKind = "create" | "update" | "rewrite" | "delete";

/** 单个 ref 的待推送内容。 */
export interface RefPreview {
  localRef: string;
  remoteRef: string;
  kind: RefKind;
  commits: CommitInfo[];
}

/** 待用户确认的推送请求。 */
export interface PushConfirmation {
  requestId: string;
  repositoryPath: string;
  remoteName: string;
  remoteUrl: string;
  refs: RefPreview[];
  timeoutSeconds: number;
  receivedAtEpochSecs: number;
}

/** 全局设置：只影响未登记的仓库。 */
export interface GlobalSettings {
  requireConfirmationForUnregisteredRepositories: boolean;
  confirmationTimeoutSeconds: number;
}

/** 单个仓库的设置；`hookStatus` 是稳定状态码，由前端翻译。 */
export interface RepositorySettings {
  repositoryId: string;
  canonicalPath: string;
  pushConfirmationEnabled: boolean;
  hookStatus: string;
}

/** 设置页需要的全部数据。 */
export interface Settings {
  global: GlobalSettings;
  repositories: RepositorySettings[];
  configError: string | null;
  ipcError: string | null;
  /** 桌面端实际读写的配置文件路径；只读展示，前端不拼接也不猜测。 */
  configPath: string;
}

/** command 失败时返回的稳定错误码。 */
export class GitGuardCommandError extends Error {
  constructor(readonly code: string) {
    super(code);
    this.name = "GitGuardCommandError";
  }
}

/** 单条审计事件；审计由 CLI 写入，桌面端只读。 */
export interface AuditEvent {
  timestampEpochSecs: number;
  /** 与时间戳等价的 UTC 文本，由 Rust 侧格式化。 */
  timestamp: string;
  event: AuditEventCode;
  result: AuditResultCode;
  reasonCode: string;
  channel: AuditChannelCode;
  repositoryPath: string;
  repositoryId: string | null;
  remoteName: string;
  /** Remote URL，已由 Rust 侧隐去凭据。 */
  remoteUrl: string;
  requestId: string | null;
  refCount: number;
}

/** 审计页需要的全部数据；事件按「新 → 旧」排列。 */
export interface AuditLog {
  path: string;
  retentionDays: number;
  maxEntries: number;
  events: AuditEvent[];
  skippedLines: number;
  truncated: boolean;
  /** 审计文件读取失败时的稳定错误码。 */
  loadError: string | null;
  configError: string | null;
  ipcError: string | null;
}

const REF_KINDS: readonly RefKind[] = ["create", "update", "rewrite", "delete"];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function parseRefKind(value: unknown): RefKind | null {
  return REF_KINDS.find((kind) => kind === value) ?? null;
}

function parseCommitInfo(value: unknown): CommitInfo | null {
  if (!isRecord(value)) {
    return null;
  }
  const { sha, shortSha, author, timestamp, subject } = value;
  if (
    typeof sha !== "string" ||
    typeof shortSha !== "string" ||
    typeof author !== "string" ||
    typeof timestamp !== "string" ||
    typeof subject !== "string"
  ) {
    return null;
  }
  return { sha, shortSha, author, timestamp, subject };
}

function parseRefPreview(value: unknown): RefPreview | null {
  if (!isRecord(value)) {
    return null;
  }
  const kind = parseRefKind(value.kind);
  if (
    kind === null ||
    typeof value.localRef !== "string" ||
    typeof value.remoteRef !== "string" ||
    !Array.isArray(value.commits)
  ) {
    return null;
  }

  const commits: CommitInfo[] = [];
  for (const entry of value.commits) {
    const commit = parseCommitInfo(entry);
    if (commit === null) {
      return null;
    }
    commits.push(commit);
  }

  return {
    localRef: value.localRef,
    remoteRef: value.remoteRef,
    kind,
    commits,
  };
}

/** 把 command 返回值收窄为待确认请求；形状不完整时返回 null。 */
export function parsePushConfirmation(value: unknown): PushConfirmation | null {
  if (!isRecord(value)) {
    return null;
  }
  const {
    requestId,
    repositoryPath,
    remoteName,
    remoteUrl,
    refs,
    timeoutSeconds,
    receivedAtEpochSecs,
  } = value;

  if (
    typeof requestId !== "string" ||
    typeof repositoryPath !== "string" ||
    typeof remoteName !== "string" ||
    typeof remoteUrl !== "string" ||
    typeof timeoutSeconds !== "number" ||
    typeof receivedAtEpochSecs !== "number" ||
    !Array.isArray(refs)
  ) {
    return null;
  }

  const parsedRefs: RefPreview[] = [];
  for (const entry of refs) {
    const ref = parseRefPreview(entry);
    if (ref === null) {
      return null;
    }
    parsedRefs.push(ref);
  }

  return {
    requestId,
    repositoryPath,
    remoteName,
    remoteUrl,
    refs: parsedRefs,
    timeoutSeconds,
    receivedAtEpochSecs,
  };
}

function parseSettings(value: unknown): Settings | null {
  if (!isRecord(value) || !isRecord(value.global)) {
    return null;
  }

  const {
    requireConfirmationForUnregisteredRepositories,
    confirmationTimeoutSeconds,
  } = value.global;
  if (
    typeof requireConfirmationForUnregisteredRepositories !== "boolean" ||
    typeof confirmationTimeoutSeconds !== "number" ||
    !Array.isArray(value.repositories)
  ) {
    return null;
  }

  const repositories: RepositorySettings[] = [];
  for (const entry of value.repositories) {
    if (!isRecord(entry)) {
      return null;
    }
    const { repositoryId, canonicalPath, pushConfirmationEnabled, hookStatus } =
      entry;
    if (
      typeof repositoryId !== "string" ||
      typeof canonicalPath !== "string" ||
      typeof pushConfirmationEnabled !== "boolean" ||
      typeof hookStatus !== "string"
    ) {
      return null;
    }
    repositories.push({
      repositoryId,
      canonicalPath,
      pushConfirmationEnabled,
      hookStatus,
    });
  }

  if (typeof value.configPath !== "string") {
    return null;
  }

  return {
    global: {
      requireConfirmationForUnregisteredRepositories,
      confirmationTimeoutSeconds,
    },
    repositories,
    configError:
      typeof value.configError === "string" ? value.configError : null,
    ipcError: typeof value.ipcError === "string" ? value.ipcError : null,
    configPath: value.configPath,
  };
}

function parseAuditEvent(value: unknown): AuditEvent | null {
  if (!isRecord(value)) {
    return null;
  }
  const {
    timestampEpochSecs,
    timestamp,
    reasonCode,
    repositoryPath,
    remoteName,
    remoteUrl,
    refCount,
  } = value;

  if (
    typeof timestampEpochSecs !== "number" ||
    typeof timestamp !== "string" ||
    typeof reasonCode !== "string" ||
    typeof repositoryPath !== "string" ||
    typeof remoteName !== "string" ||
    typeof remoteUrl !== "string" ||
    typeof refCount !== "number" ||
    !isAuditEventCode(value.event) ||
    !isAuditResultCode(value.result) ||
    !isAuditChannelCode(value.channel)
  ) {
    return null;
  }

  return {
    timestampEpochSecs,
    timestamp,
    event: value.event,
    result: value.result,
    reasonCode,
    channel: value.channel,
    repositoryPath,
    repositoryId:
      typeof value.repositoryId === "string" ? value.repositoryId : null,
    remoteName,
    remoteUrl,
    requestId: typeof value.requestId === "string" ? value.requestId : null,
    refCount,
  };
}

/** 把 command 返回值收窄为审计页数据；形状不完整时返回 null（不展示猜测内容）。 */
export function parseAuditLog(value: unknown): AuditLog | null {
  if (!isRecord(value) || !Array.isArray(value.events)) {
    return null;
  }
  const { path, retentionDays, maxEntries, skippedLines, truncated } = value;
  if (
    typeof path !== "string" ||
    typeof retentionDays !== "number" ||
    typeof maxEntries !== "number" ||
    typeof skippedLines !== "number" ||
    typeof truncated !== "boolean"
  ) {
    return null;
  }

  const events: AuditEvent[] = [];
  for (const entry of value.events) {
    const event = parseAuditEvent(entry);
    if (event === null) {
      return null;
    }
    events.push(event);
  }

  return {
    path,
    retentionDays,
    maxEntries,
    events,
    skippedLines,
    truncated,
    loadError: typeof value.loadError === "string" ? value.loadError : null,
    configError:
      typeof value.configError === "string" ? value.configError : null,
    ipcError: typeof value.ipcError === "string" ? value.ipcError : null,
  };
}

function toCommandError(reason: unknown): GitGuardCommandError {
  if (reason instanceof GitGuardCommandError) {
    return reason;
  }
  if (isRecord(reason) && typeof reason.code === "string") {
    return new GitGuardCommandError(reason.code);
  }
  return new GitGuardCommandError("unknown");
}

/** 当前是否运行在 Tauri 容器内；浏览器里直接打开 Vite 时为 false。 */
export function isDesktopAvailable(): boolean {
  return (
    typeof window !== "undefined" &&
    "__TAURI_INTERNALS__" in (window as unknown as Record<string, unknown>)
  );
}

/** 读取当前待确认的推送请求；没有或形状不对时返回 null。 */
export async function getPendingPush(): Promise<PushConfirmation | null> {
  const value = await invoke<unknown>("get_pending_push_request");
  return parsePushConfirmation(value);
}

/**
 * 用户确认推送；失败时抛 {@link GitGuardCommandError}。
 *
 * 桌面端只把「已确认」回给 CLI，真正的推送由原始 `git push` 进程继续执行，
 * 所以这里没有推送结果可展示，成功时只返回 void。
 */
export async function confirmPush(requestId: string): Promise<void> {
  try {
    await invoke("confirm_push", { requestId });
  } catch (reason) {
    throw toCommandError(reason);
  }
}

/** 用户取消推送。 */
export async function cancelPush(requestId: string): Promise<void> {
  try {
    await invoke("cancel_push", { requestId });
  } catch (reason) {
    throw toCommandError(reason);
  }
}

/** 读取设置页数据。 */
export async function getSettings(): Promise<Settings> {
  const value = await invoke<unknown>("get_settings");
  const settings = parseSettings(value);
  if (settings === null) {
    throw new GitGuardCommandError("internal_error");
  }
  return settings;
}

/** 读取审计页数据；返回形状不对时抛 `internal_error`，不展示猜测内容。 */
export async function getAuditLog(): Promise<AuditLog> {
  const value = await invoke<unknown>("get_audit_log");
  const log = parseAuditLog(value);
  if (log === null) {
    throw new GitGuardCommandError("internal_error");
  }
  return log;
}

/**
 * 生成诊断摘要文本（已由 Rust 侧脱敏），供界面复制。
 *
 * `lastErrorCode` 只用于把用户刚遇到的报错与审计事件对上；
 * 它不是句子，后端会再按字符集收窄一次。
 */
export async function buildDiagnostics(
  lastErrorCode: string | null,
): Promise<string> {
  try {
    const value = await invoke<unknown>("build_diagnostics", { lastErrorCode });
    if (typeof value !== "string") {
      throw new GitGuardCommandError("internal_error");
    }
    return value;
  } catch (reason) {
    throw toCommandError(reason);
  }
}

/** 把诊断摘要导出到配置目录，返回实际路径。 */
export async function exportDiagnostics(
  lastErrorCode: string | null,
): Promise<string> {
  try {
    const value = await invoke<unknown>("export_diagnostics", {
      lastErrorCode,
    });
    if (typeof value !== "string") {
      throw new GitGuardCommandError("internal_error");
    }
    return value;
  } catch (reason) {
    throw toCommandError(reason);
  }
}

/**
 * 把文本写进剪贴板；环境不允许时返回 false，由界面提示改用导出。
 *
 * 剪贴板不是安全控制：摘要已经由 Rust 侧脱敏，这里只做写入。
 */
export async function copyText(text: string): Promise<boolean> {
  const clipboard: Clipboard | undefined = globalThis.navigator?.clipboard;
  if (!clipboard) {
    return false;
  }
  try {
    await clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/** 添加仓库并安装 Hook；`enabled` 是该仓库的推送保护开关初始值。 */
export async function addRepository(
  path: string,
  enabled: boolean,
): Promise<void> {
  try {
    await invoke("add_repository", { path, enabled });
  } catch (reason) {
    throw toCommandError(reason);
  }
}

/**
 * 打开系统文件夹选择器并返回用户选中的目录。
 *
 * 取消选择（返回 null）或返回非字符串时一律当作「没有选择」，
 * 不使用上一次的输入顶替，也不猜测路径。
 */
export async function pickRepositoryDirectory(): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}

/** 切换仓库的推送保护开关。 */
export async function toggleProtection(
  repositoryId: string,
  enabled: boolean,
): Promise<void> {
  try {
    await invoke("toggle_repo_protection", { repositoryId, enabled });
  } catch (reason) {
    throw toCommandError(reason);
  }
}

/** 修改未登记的仓库是否需要推送确认。 */
export async function setUnregisteredProtection(
  enabled: boolean,
): Promise<void> {
  try {
    await invoke("set_unregistered_protection", { enabled });
  } catch (reason) {
    throw toCommandError(reason);
  }
}

/** 设置确认超时时间（秒）。 */
export async function setConfirmationTimeout(seconds: number): Promise<void> {
  try {
    await invoke("set_confirmation_timeout", { seconds });
  } catch (reason) {
    throw toCommandError(reason);
  }
}

/**
 * 让 Rust 侧按当前界面语言重建原生菜单。
 *
 * 菜单是展示层：失败只影响菜单文案，不影响任何确认或放行判断。
 */
export async function setAppMenu(locale: string): Promise<void> {
  try {
    await invoke("set_app_menu", { locale });
  } catch (reason) {
    throw toCommandError(reason);
  }
}

/** 监听原生菜单里的「设置…」；返回取消监听的函数。 */
export async function listenOpenSettings(
  handler: () => void,
): Promise<() => void> {
  const unlisten = await listen("open-settings", () => handler());
  return unlisten;
}

/**
 * 监听新的待确认请求事件。
 *
 * 返回取消监听的函数；事件负载形状不对时忽略该事件（不用它驱动确认界面）。
 */
export async function listenPushRequests(
  handler: (confirmation: PushConfirmation) => void,
): Promise<() => void> {
  const unlisten = await listen<unknown>("push-request-received", (event) => {
    const confirmation = parsePushConfirmation(event.payload);
    if (confirmation !== null) {
      handler(confirmation);
    }
  });
  return unlisten;
}
