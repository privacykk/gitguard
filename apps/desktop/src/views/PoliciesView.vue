<script setup lang="ts">
/**
 * 安全策略：只调整「哪些仓库需要确认」，并把判定顺序讲清楚。
 *
 * 页面上的开关只是提交请求，放行决定始终来自 Rust；
 * 按仓库的开关在这里只读汇总，修改入口统一指向仓库管理。
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { Settings } from "../api";
import CardGrid from "../components/ui/CardGrid.vue";
import NoticeBanner from "../components/ui/NoticeBanner.vue";
import PlannedList from "../components/ui/PlannedList.vue";
import RepoRow from "../components/ui/RepoRow.vue";
import PageHeader from "../components/ui/PageHeader.vue";
import SettingRow from "../components/ui/SettingRow.vue";
import StepList from "../components/ui/StepList.vue";
import SurfaceCard from "../components/ui/SurfaceCard.vue";
import type { AppView } from "../navigation";

const props = defineProps<{
  settings: Settings | null;
  busy: boolean;
}>();

const emit = defineEmits<{
  setUnregisteredProtection: [enabled: boolean];
  navigate: [view: AppView];
}>();

const { t } = useI18n();

const repositories = computed(() => props.settings?.repositories ?? []);
/** 设置没取到时开关状态是「未知」，不能显示成「关闭」再让人点。 */
const settingsReady = computed(() => props.settings !== null);
const unregisteredProtection = computed(
  () =>
    props.settings?.global.requireConfirmationForUnregisteredRepositories ??
    false,
);

const orderSteps = computed(() => [
  t("policies.orderStep1"),
  t("policies.orderStep2"),
  t("policies.orderStep3"),
]);

const plannedItems = computed(() => [
  {
    title: t("policies.plannedPriority"),
    body: t("policies.plannedPriorityBody"),
  },
  {
    title: t("policies.plannedBinding"),
    body: t("policies.plannedBindingBody"),
  },
]);

/** 开关只发出请求，真实状态始终来自 Rust 返回的配置。 */
function requestUnregisteredProtection(enabled: string | number | boolean): void {
  // el-switch 允许自定义 active-value；这里只处理布尔开关。
  if (typeof enabled !== "boolean") {
    return;
  }
  emit("setUnregisteredProtection", enabled);
}
</script>

<template>
  <section aria-labelledby="policies-title">
    <PageHeader
      title-id="policies-title"
      :title="t('policies.title')"
      :subtitle="t('policies.subtitle')"
    />

    <NoticeBanner
      v-if="!settingsReady"
      type="warning"
      :description="t('common.loadFailed')"
      class="mb-4"
    />

    <CardGrid>
      <SurfaceCard
        :title="t('policies.globalTitle')"
        :description="t('policies.globalHint')"
      >
        <SettingRow :label="t('policies.unregisteredProtection')">
          <div class="flex items-center gap-2">
            <span
              v-if="!settingsReady"
              class="text-caption text-ink-subtle"
            >
              {{ t("common.unknown") }}
            </span>
            <el-switch
              :model-value="unregisteredProtection"
              :disabled="busy || !settingsReady"
              :aria-label="t('policies.unregisteredProtection')"
              data-unregistered-switch
              @update:model-value="requestUnregisteredProtection"
            />
          </div>
        </SettingRow>
      </SurfaceCard>

      <SurfaceCard :title="t('policies.orderTitle')">
        <StepList :steps="orderSteps" />
      </SurfaceCard>

      <SurfaceCard
        :title="t('policies.overridesTitle')"
        :description="t('policies.overridesHint')"
      >
        <ul
          v-if="repositories.length > 0"
          class="m-0 list-none p-0"
        >
          <RepoRow
            v-for="repository in repositories"
            :key="repository.repositoryId"
            :repository="repository"
          />
        </ul>
        <p
          v-else-if="!settingsReady"
          class="m-0 text-meta text-ink-muted"
        >
          {{ t("common.loadFailed") }}
        </p>
        <p
          v-else
          class="m-0 text-meta text-ink-muted"
        >
          {{ t("policies.overridesEmpty") }}
        </p>
        <template #actions>
          <el-button
            size="small"
            @click="emit('navigate', 'repositories')"
          >
            {{ t("policies.goToRepositories") }}
          </el-button>
        </template>
      </SurfaceCard>

      <SurfaceCard :title="t('policies.channelTitle')">
        <p class="m-0 text-meta leading-relaxed text-ink-muted">
          {{ t("policies.channelBody") }}
        </p>
        <p class="m-0 text-meta leading-relaxed text-ink-muted">
          {{ t("policies.timeoutHint") }}
        </p>
        <template #actions>
          <el-button
            size="small"
            @click="emit('navigate', 'settings')"
          >
            {{ t("policies.openSettings") }}
          </el-button>
        </template>
      </SurfaceCard>

      <SurfaceCard
        :title="t('policies.plannedTitle')"
        tone="planned"
      >
        <PlannedList :items="plannedItems" />
      </SurfaceCard>
    </CardGrid>
  </section>
</template>
