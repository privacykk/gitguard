<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import {
  isDesktopAvailable,
  pickRepositoryDirectory,
  type Settings,
} from "../api";
import { toCommandErrorMessageKey, toHookStatusMessageKey } from "../statusCodes";

const props = defineProps<{
  settings: Settings | null;
  busy: boolean;
  /** 「超时时间已保存」提示由父组件在后端写入成功后置位，失败时不谎报成功。 */
  timeoutSaved: boolean;
}>();

const emit = defineEmits<{
  add: [path: string, enabled: boolean];
  toggle: [repositoryId: string, enabled: boolean];
  setDefaultProtection: [enabled: boolean];
  saveTimeout: [seconds: number];
  failed: [reason: unknown];
}>();

const { t } = useI18n();

/** 文件夹选择依赖 Tauri 插件，浏览器里直接打开 Vite 时隐藏该按钮。 */
const desktopAvailable = isDesktopAvailable();

const pathInput = ref("");
// `<input type="number">` 的 v-model 会把输入自动转成数字，因此这里两种类型都要接受。
const timeoutInput = ref<string | number>("180");
/** 用户修改过输入框：隐藏上一次的「已保存」，避免提示与当前值不一致。 */
const timeoutEdited = ref(false);
const showTimeoutSaved = computed(
  () => props.timeoutSaved && !timeoutEdited.value,
);
const picking = ref(false);
/** 添加仓库时是否启用推送保护；默认跟随全局设置，可由用户当场改写。 */
const enableOnAdd = ref(true);

// 后端返回新设置后同步输入框，避免界面显示过期值。
watch(
  () => props.settings?.global.confirmationTimeoutSeconds,
  (seconds) => {
    if (typeof seconds === "number") {
      timeoutInput.value = String(seconds);
      timeoutEdited.value = false;
    }
  },
  { immediate: true },
);

// 全局默认值变化时，添加表单的默认勾选也跟着变。
watch(
  () => props.settings?.global.defaultPushConfirmation,
  (enabled) => {
    if (typeof enabled === "boolean") {
      enableOnAdd.value = enabled;
    }
  },
  { immediate: true },
);

const repositories = computed(() => props.settings?.repositories ?? []);
const canAdd = computed(() => pathInput.value.trim().length > 0 && !props.busy);
const defaultProtection = computed(
  () => props.settings?.global.defaultPushConfirmation ?? false,
);

/** 解析超时输入；非整数或越界时返回 null，交给后端拒绝而不是本地猜测。 */
const parsedTimeout = computed<number | null>(() => {
  const raw = timeoutInput.value;
  const value = typeof raw === "number" ? raw : Number(raw.trim());
  return Number.isInteger(value) && value >= 1 && value <= 3600 ? value : null;
});

/** 打开系统文件夹选择器；用户取消时保持输入框内容不变。 */
async function browse(): Promise<void> {
  if (picking.value || props.busy) {
    return;
  }

  picking.value = true;
  try {
    const directory = await pickRepositoryDirectory();
    if (directory !== null) {
      pathInput.value = directory;
    }
  } catch (reason) {
    emit("failed", reason);
  } finally {
    picking.value = false;
  }
}

function submitAdd() {
  if (!canAdd.value) {
    return;
  }
  // 是否启用保护由添加表单上的勾选项决定，不在后端硬编码。
  emit("add", pathInput.value.trim(), enableOnAdd.value);
}

function submitTimeout() {
  if (parsedTimeout.value === null || props.busy) {
    return;
  }
  // 提示的显示与否交给父组件根据命令结果决定。
  timeoutEdited.value = false;
  emit("saveTimeout", parsedTimeout.value);
}
</script>

<template>
  <section class="settings" aria-labelledby="settings-title">
    <h2 id="settings-title">
      {{ t("settings.title") }}
    </h2>
    <p class="settings__intro">
      {{ t("settings.intro") }}
    </p>

    <p v-if="settings?.configError" class="settings__error" role="alert">
      {{ t(toCommandErrorMessageKey(settings.configError)) }}
    </p>
    <p v-if="settings?.ipcError" class="settings__error" role="alert">
      {{ t(toCommandErrorMessageKey(settings.ipcError)) }}
    </p>

    <h3 class="settings__heading">
      {{ t("settings.repositories") }}
    </h3>
    <p v-if="repositories.length === 0" class="settings__empty">
      {{ t("settings.empty") }}
    </p>
    <ul v-else class="settings__list">
      <li v-for="repository in repositories" :key="repository.repositoryId" class="settings__item">
        <div class="settings__repository">
          <code class="settings__path">{{ repository.canonicalPath }}</code>
          <span class="settings__hook-status" :class="`settings__hook-status--${repository.hookStatus.toLowerCase()}`">
            {{ t("settings.hookStatusLabel") }}:
            {{ t(toHookStatusMessageKey(repository.hookStatus)) }}
          </span>
        </div>
        <button type="button" class="settings__switch"
          :class="{ 'settings__switch--on': repository.pushConfirmationEnabled }" role="switch"
          :aria-checked="repository.pushConfirmationEnabled" :aria-label="t('settings.toggleLabel', { path: repository.canonicalPath })
            " :disabled="busy" @click="
            emit(
              'toggle',
              repository.repositoryId,
              !repository.pushConfirmationEnabled,
            )
            ">
          <span class="settings__switch-track" aria-hidden="true" />
          <span class="settings__switch-text">
            {{
              repository.pushConfirmationEnabled
                ? t("settings.enabled")
                : t("settings.disabled")
            }}
          </span>
        </button>
      </li>
    </ul>

    <form class="settings__form" @submit.prevent="submitAdd">
      <label class="settings__label" for="repository-path">
        {{ t("settings.addLabel") }}
      </label>
      <div class="settings__row">
        <input id="repository-path" v-model="pathInput" class="settings__input" type="text" autocomplete="off"
          :placeholder="t('settings.addPlaceholder')" :disabled="busy">
        <button v-if="desktopAvailable" type="button" class="settings__button settings__button--secondary"
          :disabled="busy || picking" @click="browse">
          {{ picking ? t("settings.browsing") : t("settings.browseButton") }}
        </button>
        <button type="submit" class="settings__button" :disabled="!canAdd">
          {{ busy ? t("settings.adding") : t("settings.addButton") }}
        </button>
      </div>
      <label class="settings__checkbox">
        <input v-model="enableOnAdd" type="checkbox" :disabled="busy">
        <span>{{ t("settings.addWithProtection") }}</span>
      </label>
    </form>

    <form class="settings__form" @submit.prevent="submitTimeout">
      <label class="settings__label" for="confirmation-timeout">
        {{ t("settings.timeoutLabel") }}
      </label>
      <div class="settings__row">
        <input id="confirmation-timeout" v-model="timeoutInput" class="settings__input settings__input--number"
          type="number" min="1" max="3600" step="1" :disabled="busy" @input="timeoutEdited = true">
        <button type="submit" class="settings__button" :disabled="parsedTimeout === null || busy">
          {{ t("settings.timeoutSave") }}
        </button>
        <span v-if="showTimeoutSaved" class="settings__saved" role="status">
          {{ t("settings.timeoutSaved") }}
        </span>
      </div>
    </form>

    <div v-if="settings" class="settings__default">
      <span class="settings__label">{{ t("settings.defaultProtection") }}</span>
      <button type="button" class="settings__switch" :class="{ 'settings__switch--on': defaultProtection }"
        role="switch" :aria-checked="defaultProtection" :disabled="busy"
        @click="emit('setDefaultProtection', !defaultProtection)">
        <span class="settings__switch-track" aria-hidden="true" />
        <span class="settings__switch-text">
          {{ defaultProtection ? t("settings.enabled") : t("settings.disabled") }}
        </span>
      </button>
    </div>
  </section>
</template>

<style scoped>
.settings {
  padding: 1.5rem;
  border: 1px solid #d7dee8;
  border-radius: 0.75rem;
  background: #ffffff;
}

.settings__intro,
.settings__empty {
  color: #55627a;
}

.settings__error {
  padding: 0.5rem 0.75rem;
  border: 1px solid #f0c4c4;
  border-radius: 0.375rem;
  background: #fdf5f5;
  color: #a12222;
}

.settings__heading {
  margin-bottom: 0.5rem;
  font-size: 1rem;
}

.settings__list {
  margin: 0;
  padding: 0;
  list-style: none;
}

.settings__item {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  align-items: center;
  justify-content: space-between;
  padding: 0.75rem 0;
  border-bottom: 1px solid #e6ebf2;
}

.settings__repository {
  display: grid;
  gap: 0.25rem;
}

.settings__path {
  overflow-wrap: anywhere;
  font-family: ui-monospace, SFMono-Regular, monospace;
  font-size: 0.875rem;
}

.settings__hook-status {
  font-size: 0.8125rem;
  color: #55627a;
}

.settings__hook-status--healthy {
  color: #1c6b3a;
}

.settings__hook-status--conflict,
.settings__hook-status--modified {
  color: #a12222;
}

.settings__switch {
  display: inline-flex;
  gap: 0.5rem;
  align-items: center;
  padding: 0.25rem 0.625rem 0.25rem 0.25rem;
  border: 1px solid #d7dee8;
  border-radius: 999px;
  background: #ffffff;
  cursor: pointer;
  font-size: 0.8125rem;
}

.settings__switch:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

.settings__switch-track {
  position: relative;
  width: 2rem;
  height: 1.125rem;
  border-radius: 999px;
  background: #c3cbe0;
  transition: background 0.15s ease;
}

.settings__switch-track::after {
  content: "";
  position: absolute;
  top: 0.125rem;
  left: 0.125rem;
  width: 0.875rem;
  height: 0.875rem;
  border-radius: 50%;
  background: #ffffff;
  transition: transform 0.15s ease;
}

.settings__switch--on .settings__switch-track {
  background: #3b5ccc;
}

.settings__switch--on .settings__switch-track::after {
  transform: translateX(0.875rem);
}

.settings__switch-text {
  color: #55627a;
}

.settings__switch--on .settings__switch-text {
  color: #1c6b3a;
}

.settings__button {
  padding: 0.375rem 0.75rem;
  border: 1px solid #3b5ccc;
  border-radius: 0.375rem;
  background: #3b5ccc;
  color: #ffffff;
  cursor: pointer;
}

.settings__button--secondary {
  background: #ffffff;
  border-color: #d7dee8;
  color: #1f2937;
}

.settings__button:disabled {
  background: #c3cbe0;
  border-color: #c3cbe0;
  color: #ffffff;
  cursor: not-allowed;
}

.settings__form {
  display: grid;
  gap: 0.375rem;
  margin-top: 1rem;
}

.settings__label {
  font-size: 0.875rem;
}

.settings__row {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  align-items: center;
}

.settings__input {
  flex: 1 1 16rem;
  padding: 0.5rem;
  border: 1px solid #d7dee8;
  border-radius: 0.375rem;
}

.settings__input--number {
  flex: 0 0 6rem;
}

.settings__checkbox {
  display: inline-flex;
  gap: 0.375rem;
  align-items: center;
  color: #55627a;
  font-size: 0.875rem;
}

.settings__saved {
  color: #1c6b3a;
  font-size: 0.8125rem;
}

.settings__default {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  align-items: center;
  justify-content: space-between;
  margin-top: 1.25rem;
  padding-top: 1rem;
  border-top: 1px solid #e6ebf2;
}
</style>
