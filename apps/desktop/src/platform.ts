/**
 * 平台判断只用于快捷键的**展示**。
 *
 * 按键处理同时接受 Meta 与 Ctrl，所以平台判断错了也不会让快捷键失效，
 * 最多是提示文案与用户习惯不一致。
 */
export function isMacPlatform(): boolean {
  if (typeof navigator === "undefined") {
    return false;
  }
  const { platform, userAgent } = navigator;
  if (typeof platform === "string" && platform.length > 0) {
    return platform.toLowerCase().includes("mac");
  }
  return /Macintosh|Mac OS X/i.test(userAgent);
}

/** 设置页快捷键文案：macOS 用 ⌘,，其它系统用市面上通用的 Ctrl+,。 */
export function settingsShortcutLabel(): string {
  return isMacPlatform() ? "⌘+," : "Ctrl+,";
}
