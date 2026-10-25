<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import {
  addRepository,
  cancelPush,
  confirmPush,
  getPendingPush,
  getSettings,
  isDesktopAvailable,
  listenOpenSettings,
  listenPushRequests,
  setAppMenu,
  setConfirmationTimeout,
  setUnregisteredProtection,
  toggleProtection,
  type PushConfirmation,
  type Settings,
} from "./api";
import AppSidebar from "./components/AppSidebar.vue";
import NoticeBanner from "./components/ui/NoticeBanner.vue";
import type { AppView } from "./navigation";
import { commandErrorCode, toCommandErrorMessageKey } from "./statusCodes";
import ApprovalView from "./views/ApprovalView.vue";
import AuditView from "./views/AuditView.vue";
import DashboardView from "./views/DashboardView.vue";
import IdentitiesView from "./views/IdentitiesView.vue";
import PoliciesView from "./views/PoliciesView.vue";
import RepositoriesView from "./views/RepositoriesView.vue";
import SettingsView from "./views/SettingsView.vue";

const { t, locale } = useI18n();

/** 确认成功提示的停留时长（毫秒），避免提示看起来像常驻状态。 */
const CONFIRMED_NOTICE_MS = 8000;

/** 侧边栏收起状态的本地记忆键；纯视觉偏好，不参与任何安全判断。 */
const SIDEBAR_STORAGE_KEY = "gitguard.sidebarCollapsed";

/** 浏览器里直接打开 Vite 时没有 Tauri 后端，此时页面只读。 */
const desktopAvailable = isDesktopAvailable();
const loading = ref(desktopAvailable);
const busy = ref(false);
/** 收起后侧边栏只留图标，给内容区让出宽度。 */
const sidebarCollapsed = ref(readSidebarCollapsed());

/** 当前页面与设置页的来源页；设置页据此实现 Esc 返回。 */
const view = ref<AppView>("dashboard");
const previousView = ref<AppView>("dashboard");

/** 当前的待确认请求；非空时界面提供确认入口。 */
const pending = ref<PushConfirmation | null>(null);
const appSettings = ref<Settings | null>(null);
/** 上一次确认是否已回给 CLI；为 true 时显示「原 git push 将继续」的提示。 */
const confirmed = ref(false);
/** 「确认超时时间已保存」提示；只有后端写入成功才置位，失败不谎报成功。 */
const timeoutSaved = ref(false);
/** 稳定错误码；有值时显示 fail-closed 提示，绝不显示成「已放行」。 */
const errorKey = ref<string | null>(null);

/** 顶部提示区是否需要占位：有任一提示时才渲染容器，避免空 div 留白。 */
const hasNotice = computed(
  () =>
    !desktopAvailable ||
    errorKey.value !== null ||
    confirmed.value ||
    (pending.value !== null && view.value !== "approval"),
);

let pollTimer: number | undefined;
let noticeTimer: number | undefined;
let stopListening: (() => void) | undefined;
let stopMenuListening: (() => void) | undefined;

function setError(reason: unknown): void {
  errorKey.value = toCommandErrorMessageKey(commandErrorCode(reason));
}

/** 读取上次的收起状态；存储不可用时按展开处理。 */
function readSidebarCollapsed(): boolean {
  try {
    return window.localStorage.getItem(SIDEBAR_STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

/** 切换侧边栏展开/收起，并记住选择。 */
function toggleSidebar(): void {
  sidebarCollapsed.value = !sidebarCollapsed.value;
  try {
    window.localStorage.setItem(
      SIDEBAR_STORAGE_KEY,
      String(sidebarCollapsed.value),
    );
  } catch {
    // 存储不可用只影响下次打开的默认状态，不阻断当前操作。
  }
}

/**
 * 切换页面。
 *
 * 只有离开业务页面时才更新来源页，因此从设置页跳到确认页再回来不会丢失原位置。
 */
function navigate(next: AppView): void {
  if (next === view.value) {
    return;
  }
  if (view.value !== "settings") {
    previousView.value = view.value;
  }
  if (next !== "settings") {
    timeoutSaved.value = false;
  }
  view.value = next;
}

function openSettings(): void {
  navigate("settings");
}

function closeSettings(): void {
  const target =
    previousView.value === "settings" ? "dashboard" : previousView.value;
  navigate(target);
}

/** macOS 的 ⌘, 与其它系统的 Ctrl+, 同时接受，避免平台判断出错时快捷键失效。 */
function isSettingsShortcut(event: KeyboardEvent): boolean {
  return (
    (event.metaKey || event.ctrlKey) &&
    !event.altKey &&
    (event.key === "," || event.code === "Comma")
  );
}

function handleKeydown(event: KeyboardEvent): void {
  if (isSettingsShortcut(event)) {
    event.preventDefault();
    openSettings();
    return;
  }
  if (event.key === "Escape" && view.value === "settings") {
    event.preventDefault();
    closeSettings();
  }
}

/** 取回设置页数据；失败时由调用方决定是否展示错误。 */
async function loadSettings(): Promise<void> {
  appSettings.value = await getSettings();
}

/**
 * 应用一次待确认请求。
 *
 * 新请求有时效性：无论用户停在哪一页都切到确认页；已处理的请求不改变当前页面。
 */
function applyPending(next: PushConfirmation | null): void {
  if (next !== null && next.requestId !== pending.value?.requestId) {
    confirmed.value = false;
    errorKey.value = null;
    navigate("approval");
  }
  pending.value = next;
}

/** 轮询待确认请求：既用于初次加载，也用于发现超时后的自动关闭。 */
async function refreshPending(): Promise<void> {
  try {
    applyPending(await getPendingPush());
  } catch {
    // 读取失败时不改变界面状态：真正的拒绝由 Hook 侧的 fail-closed 保证。
  }
}

onMounted(async () => {
  window.addEventListener("keydown", handleKeydown);

  if (!desktopAvailable) {
    loading.value = false;
    return;
  }

  try {
    await Promise.all([refreshPending(), loadSettings()]);
  } catch (reason) {
    setError(reason);
  } finally {
    loading.value = false;
  }

  pollTimer = window.setInterval(() => {
    void refreshPending();
  }, 1000);

  try {
    stopListening = await listenPushRequests(applyPending);
  } catch {
    // 事件通道不可用时仍靠轮询，功能不受影响。
  }

  try {
    stopMenuListening = await listenOpenSettings(openSettings);
  } catch {
    // 菜单事件不可用时仍可用 ⌘, / Ctrl+, 打开设置。
  }
});

onUnmounted(() => {
  window.removeEventListener("keydown", handleKeydown);
  if (pollTimer !== undefined) {
    window.clearInterval(pollTimer);
  }
  if (noticeTimer !== undefined) {
    window.clearTimeout(noticeTimer);
  }
  stopListening?.();
  stopMenuListening?.();
});

/** 语言切换后重建原生菜单，让菜单栏与界面保持一致。 */
watch(
  locale,
  (value) => {
    if (!desktopAvailable) {
      return;
    }
    // 菜单只是展示层：重建失败不影响界面与任何安全判断。
    void setAppMenu(value).catch(() => undefined);
  },
  { immediate: true },
);

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

/** 修改「未登记的仓库是否需要推送确认」；不影响已登记仓库。 */
async function handleSetUnregisteredProtection(enabled: boolean): Promise<void> {
  busy.value = true;
  errorKey.value = null;
  try {
    await setUnregisteredProtection(enabled);
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
  <div class="flex h-screen overflow-hidden bg-canvas text-ink">
    <AppSidebar
      :current="view"
      :pending="pending !== null"
      :collapsed="sidebarCollapsed"
      @navigate="navigate"
      @toggle-collapse="toggleSidebar"
    />

    <main class="min-w-0 flex-1 overflow-y-auto">
      <div class="mx-auto w-full max-w-7xl px-6 py-7 lg:px-10">
        <div
          v-if="hasNotice"
          class="mb-4 grid gap-2"
        >
          <NoticeBanner
            v-if="!desktopAvailable"
            type="info"
            :description="t('app.desktopUnavailable')"
          />

          <NoticeBanner
            v-if="errorKey"
            type="error"
            :description="t(errorKey)"
          />
          <NoticeBanner
            v-else-if="confirmed"
            type="success"
            :description="t('push.confirmed')"
          />

          <NoticeBanner
            v-if="pending && view !== 'approval'"
            type="warning"
            :title="t('approval.pendingTitle')"
            :description="t('approval.pendingBody')"
            data-pending-banner
          >
            <template #action>
              <el-button
                size="small"
                @click="navigate('approval')"
              >
                {{ t("approval.goToApproval") }}
              </el-button>
            </template>
          </NoticeBanner>
        </div>

        <el-skeleton
          v-if="loading"
          :rows="6"
          animated
        />
        <template v-else>
          <DashboardView
            v-if="view === 'dashboard'"
            :settings="appSettings"
            :pending="pending"
            :desktop-available="desktopAvailable"
            @navigate="navigate"
          />
          <IdentitiesView v-else-if="view === 'identities'" />
          <RepositoriesView
            v-else-if="view === 'repositories'"
            :settings="appSettings"
            :busy="busy"
            @add="handleAddRepository"
            @toggle="handleToggleProtection"
            @failed="setError"
          />
          <PoliciesView
            v-else-if="view === 'policies'"
            :settings="appSettings"
            :busy="busy"
            @set-unregistered-protection="handleSetUnregisteredProtection"
            @navigate="navigate"
          />
          <ApprovalView
            v-else-if="view === 'approval'"
            :pending="pending"
            :busy="busy"
            :desktop-available="desktopAvailable"
            @confirm="handleConfirm"
            @cancel="handleCancel"
            @navigate="navigate"
          />
          <AuditView v-else-if="view === 'audit'" />
          <SettingsView
            v-else
            :settings="appSettings"
            :busy="busy"
            :timeout-saved="timeoutSaved"
            @save-timeout="handleSaveTimeout"
            @close="closeSettings"
          />
        </template>
      </div>
    </main>
  </div>
</template>
