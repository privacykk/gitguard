<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import {
  addRepository,
  cancelPush,
  confirmPush,
  getPendingPush,
  getSettings,
  isDesktopAvailable,
  listenPushRequests,
  setConfirmationTimeout,
  setDefaultProtection,
  toggleProtection,
  type PushConfirmation,
  type Settings,
} from "./api";
import LanguageSwitch from "./components/LanguageSwitch.vue";
import PushConfirmationPanel from "./components/PushConfirmation.vue";
import RepoSettings from "./components/RepoSettings.vue";
import { commandErrorCode, toCommandErrorMessageKey } from "./statusCodes";

const { t } = useI18n();

/** 确认成功提示的停留时长（毫秒），避免提示看起来像常驻状态。 */
const CONFIRMED_NOTICE_MS = 8000;

/** 浏览器里直接打开 Vite 时没有 Tauri 后端，此时页面只读。 */
const desktopAvailable = isDesktopAvailable();
const loading = ref(desktopAvailable);
const busy = ref(false);

/** 当前的待确认请求；非空时界面切换到确认页。 */
const pending = ref<PushConfirmation | null>(null);
const settings = ref<Settings | null>(null);
/** 上一次确认是否已回给 CLI；为 true 时显示「原 git push 将继续」的提示。 */
const confirmed = ref(false);
/** 「确认超时时间已保存」提示；只有后端写入成功才置位，失败不谎报成功。 */
const timeoutSaved = ref(false);
/** 稳定错误码；有值时显示 fail-closed 提示，绝不显示成「已放行」。 */
const errorKey = ref<string | null>(null);

let pollTimer: number | undefined;
let noticeTimer: number | undefined;
let stopListening: (() => void) | undefined;

async function loadSettings(): Promise<void> {
  settings.value = await getSettings();
}

/** 轮询待确认请求：既用于初次加载，也用于发现超时后的自动关闭。 */
async function refreshPending(): Promise<void> {
  try {
    const next = await getPendingPush();
    if (next !== null && next.requestId !== pending.value?.requestId) {
      // 新请求到来时清掉上一次的提示，避免旧信息干扰本次确认。
      confirmed.value = false;
      errorKey.value = null;
    }
    pending.value = next;
  } catch {
    // 读取失败时不改变界面状态：真正的拒绝由 Hook 侧的 fail-closed 保证。
  }
}

onMounted(async () => {
  if (!desktopAvailable) {
    loading.value = false;
    return;
  }

  await Promise.all([refreshPending(), loadSettings().catch(() => undefined)]);
  loading.value = false;

  pollTimer = window.setInterval(() => {
    void refreshPending();
  }, 1000);

  try {
    stopListening = await listenPushRequests((confirmation) => {
      pending.value = confirmation;
      confirmed.value = false;
      errorKey.value = null;
    });
  } catch {
    // 事件通道不可用时仍靠轮询，功能不受影响。
  }
});

onUnmounted(() => {
  if (pollTimer !== undefined) {
    window.clearInterval(pollTimer);
  }
  if (noticeTimer !== undefined) {
    window.clearTimeout(noticeTimer);
  }
  stopListening?.();
});

/** 显示确认成功提示，并在一段时间后自动收起。 */
function showConfirmed(): void {
  confirmed.value = true;
  if (noticeTimer !== undefined) {
    window.clearTimeout(noticeTimer);
  }
  noticeTimer = window.setTimeout(() => {
    confirmed.value = false;
    noticeTimer = undefined;
  }, CONFIRMED_NOTICE_MS);
}

function setError(reason: unknown): void {
  errorKey.value = toCommandErrorMessageKey(commandErrorCode(reason));
}

async function handleConfirm(): Promise<void> {
  const current = pending.value;
  if (current === null) {
    return;
  }

  busy.value = true;
  errorKey.value = null;
  try {
    await confirmPush(current.requestId);
    showConfirmed();
  } catch (reason) {
    setError(reason);
  } finally {
    busy.value = false;
    pending.value = null;
    await loadSettings().catch(() => undefined);
  }
}

async function handleCancel(): Promise<void> {
  const current = pending.value;
  if (current === null) {
    return;
  }

  busy.value = true;
  try {
    await cancelPush(current.requestId);
  } catch (reason) {
    setError(reason);
  } finally {
    busy.value = false;
    pending.value = null;
    await refreshPending();
  }
}

async function handleAddRepository(
  path: string,
  enabled: boolean,
): Promise<void> {
  busy.value = true;
  errorKey.value = null;
  try {
    await addRepository(path, enabled);
    await loadSettings();
  } catch (reason) {
    setError(reason);
  } finally {
    busy.value = false;
  }
}

/** 修改「新仓库默认启用推送保护」；只影响之后添加的仓库。 */
async function handleSetDefaultProtection(enabled: boolean): Promise<void> {
  busy.value = true;
  errorKey.value = null;
  try {
    await setDefaultProtection(enabled);
    await loadSettings();
  } catch (reason) {
    setError(reason);
  } finally {
    busy.value = false;
  }
}

async function handleToggleProtection(
  repositoryId: string,
  enabled: boolean,
): Promise<void> {
  busy.value = true;
  errorKey.value = null;
  try {
    await toggleProtection(repositoryId, enabled);
    await loadSettings();
  } catch (reason) {
    setError(reason);
  } finally {
    busy.value = false;
  }
}

async function handleSaveTimeout(seconds: number): Promise<void> {
  busy.value = true;
  errorKey.value = null;
  timeoutSaved.value = false;
  try {
    await setConfirmationTimeout(seconds);
    // 只有命令成功返回才提示「已保存」；失败时由错误提示接管。
    timeoutSaved.value = true;
    await loadSettings().catch(() => undefined);
  } catch (reason) {
    setError(reason);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <main class="shell">
    <header class="shell__header">
      <div>
        <h1>{{ t("app.title") }}</h1>
        <p class="shell__subtitle">
          {{ t("app.subtitle") }}
        </p>
      </div>
      <LanguageSwitch />
    </header>

    <p v-if="!desktopAvailable" class="notice" role="status">
      {{ t("app.desktopUnavailable") }}
    </p>
    <p v-else-if="loading" class="notice" role="status">
      {{ t("app.loading") }}
    </p>

    <p v-if="errorKey" class="notice notice--error" role="alert">
      {{ t(errorKey) }}
    </p>
    <p v-else-if="confirmed" class="notice notice--success" role="status">
      {{ t("push.confirmed") }}
    </p>

    <PushConfirmationPanel v-if="pending" :key="pending.requestId" :confirmation="pending" :busy="busy"
      @confirm="handleConfirm" @cancel="handleCancel" />
    <RepoSettings v-else-if="desktopAvailable" :settings="settings" :busy="busy" :timeout-saved="timeoutSaved"
      @add="handleAddRepository" @toggle="handleToggleProtection" @set-default-protection="handleSetDefaultProtection"
      @save-timeout="handleSaveTimeout" @failed="setError" />
  </main>
</template>

<style scoped>
.shell {
  max-width: 48rem;
  margin: 0 auto;
  padding: 3rem 1.5rem;
}

.shell__header {
  display: flex;
  flex-wrap: wrap;
  gap: 1rem;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: 1.5rem;
}

.shell__header h1 {
  margin: 0;
}

.shell__subtitle {
  margin: 0.25rem 0 0;
  color: #3b5ccc;
  font-weight: 600;
}

.notice {
  margin-bottom: 1rem;
  padding: 0.75rem 1rem;
  border: 1px solid #d7dee8;
  border-radius: 0.5rem;
  background: #f8fafc;
  color: #55627a;
}

.notice--error {
  border-color: #f0c4c4;
  background: #fdf5f5;
  color: #a12222;
}

.notice--success {
  border-color: #b8e0c4;
  background: #f4fbf6;
  color: #1d6b3a;
}
</style>
