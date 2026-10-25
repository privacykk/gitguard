<script setup lang="ts">
/**
 * 已登记仓库的一行：路径 + Hook 状态 + 可选开关。
 *
 * 仓库管理（可切换）、安全策略（只读汇总）、总览（前三条预览）共用这一行，
 * 避免三处各写一套「路径 + 状态 + 开关」。
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { RepositorySettings } from "../../api";
import {
    toHookStatusMessageKey,
    toHookStatusTone,
} from "../../statusCodes";
import PathText from "./PathText.vue";
import StatusTag from "./StatusTag.vue";

const props = withDefaults(
    defineProps<{
        repository: RepositorySettings;
        /** 是否渲染开关；只读场景（策略汇总、总览预览）传 false。 */
        interactive?: boolean;
        busy?: boolean;
    }>(),
    { interactive: false, busy: false },
);

const emit = defineEmits<{ toggle: [repositoryId: string, enabled: boolean] }>();

const { t } = useI18n();

const hookTone = computed(() => toHookStatusTone(props.repository.hookStatus));
const hookLabel = computed(() =>
    t(toHookStatusMessageKey(props.repository.hookStatus)),
);
const switchLabel = computed(() =>
    t("repositories.toggleLabel", { path: props.repository.canonicalPath }),
);

function handleToggle(enabled: string | number | boolean): void {
    // el-switch 的取值类型包含 active-value/inactive-value；这里只接受布尔开关。
    if (typeof enabled !== "boolean") {
        return;
    }
    emit("toggle", props.repository.repositoryId, enabled);
}
</script>

<template>
  <li class="flex flex-wrap items-center justify-between gap-3 border-b border-line-soft py-2.5 last:border-b-0">
    <div class="min-w-0 flex-1">
      <PathText :path="repository.canonicalPath" />
      <div class="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1">
        <span class="text-caption text-ink-subtle">
          {{ t("hookStatus.label") }}
        </span>
        <StatusTag
          :label="hookLabel"
          :tone="hookTone"
          :dot="false"
        />
      </div>
    </div>

    <el-switch
      v-if="interactive"
      :model-value="repository.pushConfirmationEnabled"
      :disabled="busy"
      :aria-label="switchLabel"
      data-repo-switch
      @update:model-value="handleToggle"
    />
    <StatusTag
      v-else
      :label="repository.pushConfirmationEnabled
        ? t('common.enabled')
        : t('common.disabled')
      "
      :tone="repository.pushConfirmationEnabled ? 'success' : 'info'"
      :dot="false"
    />
  </li>
</template>
