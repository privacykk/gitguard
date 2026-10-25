<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import type { Settings } from "../api";
import LanguageSwitch from "../components/LanguageSwitch.vue";
import CardGrid from "../components/ui/CardGrid.vue";
import NoticeBanner from "../components/ui/NoticeBanner.vue";
import PageHeader from "../components/ui/PageHeader.vue";
import PathText from "../components/ui/PathText.vue";
import SettingRow from "../components/ui/SettingRow.vue";
import SurfaceCard from "../components/ui/SurfaceCard.vue";
import { settingsShortcutLabel } from "../platform";

const props = defineProps<{
  settings: Settings | null;
  busy: boolean;
  /** 「超时时间已保存」提示由父组件在后端写入成功后置位，失败时不谎报成功。 */
  timeoutSaved: boolean;
}>();

const emit = defineEmits<{
  saveTimeout: [seconds: number];
  close: [];
}>();

const { t } = useI18n();

const shortcut = settingsShortcutLabel();

// `<input type="number">` 的 v-model 会把输入自动转成数字，因此这里两种类型都要接受。
const timeoutInput = ref<string | number>("180");
/** 用户修改过输入框：隐藏上一次的「已保存」，避免提示与当前值不一致。 */
const timeoutEdited = ref(false);
const showTimeoutSaved = computed(
  () => props.timeoutSaved && !timeoutEdited.value,
);

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

/** 解析超时输入；非整数或越界时返回 null，交给后端拒绝而不是本地猜测。 */
const parsedTimeout = computed<number | null>(() => {
  const raw = timeoutInput.value;
  const value = typeof raw === "number" ? raw : Number(raw.trim());
  return Number.isInteger(value) && value >= 1 && value <= 3600 ? value : null;
});

const configPath = computed(() => props.settings?.configPath ?? null);

function submitTimeout(): void {
  if (parsedTimeout.value === null || props.busy) {
    return;
  }
  // 提示的显示与否交给父组件根据命令结果决定。
  timeoutEdited.value = false;
  emit("saveTimeout", parsedTimeout.value);
}
</script>

<template>
  <section aria-labelledby="settings-title">
    <PageHeader
      title-id="settings-title"
      :title="t('settings.title')"
      :subtitle="t('settings.subtitle')"
    >
      <template #actions>
        <el-button @click="emit('close')">
          {{ t("common.back") }}
        </el-button>
      </template>
    </PageHeader>

    <NoticeBanner
      v-if="settings === null"
      type="warning"
      :description="t('common.loadFailed')"
      class="mb-4"
    />

    <CardGrid>
      <SurfaceCard
        :title="t('settings.languageTitle')"
        :description="t('settings.languageHint')"
      >
        <LanguageSwitch />
      </SurfaceCard>

      <SurfaceCard :title="t('settings.behaviorTitle')">
        <form
          data-timeout-form
          class="grid gap-2"
          @submit.prevent="submitTimeout"
        >
          <label
            class="text-meta font-medium text-ink"
            for="confirmation-timeout"
          >
            {{ t("settings.timeoutLabel") }}
          </label>
          <div class="flex flex-wrap items-center gap-2">
            <el-input
              id="confirmation-timeout"
              v-model="timeoutInput"
              type="number"
              min="1"
              max="3600"
              step="1"
              class="w-28"
              :disabled="busy"
              @input="timeoutEdited = true"
            />
            <el-button
              type="primary"
              native-type="submit"
              :disabled="parsedTimeout === null || busy"
            >
              {{ t("settings.timeoutSave") }}
            </el-button>
            <span
              v-if="showTimeoutSaved"
              role="status"
              class="text-meta text-success-600"
            >
              {{ t("settings.timeoutSaved") }}
            </span>
          </div>
          <p class="m-0 text-meta leading-relaxed text-ink-muted">
            {{ t("settings.timeoutHint") }}
          </p>
        </form>
      </SurfaceCard>

      <SurfaceCard :title="t('settings.shortcutsTitle')">
        <div class="grid gap-2">
          <SettingRow :label="t('settings.shortcutSettings')">
            <kbd
              class="rounded border border-line bg-canvas px-2 py-0.5 font-mono text-caption text-ink-muted"
            >{{ shortcut }}</kbd>
          </SettingRow>
          <SettingRow :label="t('settings.shortcutClose')">
            <kbd
              class="rounded border border-line bg-canvas px-2 py-0.5 font-mono text-caption text-ink-muted"
            >Esc</kbd>
          </SettingRow>
        </div>
        <p class="m-0 text-meta leading-relaxed text-ink-muted">
          {{ t("settings.shortcutConfirm") }}
        </p>
      </SurfaceCard>

      <SurfaceCard
        :title="t('settings.configTitle')"
        :description="t('settings.configPathHint')"
      >
        <p class="m-0 text-caption font-medium text-ink-subtle">
          {{ t("settings.configPathLabel") }}
        </p>
        <PathText
          v-if="configPath"
          :path="configPath"
        />
        <code
          v-else
          class="font-mono text-meta text-ink-muted"
        >—</code>
      </SurfaceCard>

      <SurfaceCard :title="t('settings.aboutTitle')">
        <p class="m-0 flex flex-wrap items-center gap-2 text-sm font-medium text-ink">
          {{ t("app.title") }}
          <el-tag
            type="info"
            size="small"
            effect="plain"
            round
            disable-transitions
          >
            {{ t("app.productStage") }}
          </el-tag>
        </p>
        <p class="m-0 text-meta leading-relaxed text-ink-muted">
          {{ t("settings.aboutBody") }}
        </p>
      </SurfaceCard>
    </CardGrid>

    <p class="mt-4 text-meta text-ink-muted">
      {{ t("settings.backHint") }}
    </p>
  </section>
</template>
