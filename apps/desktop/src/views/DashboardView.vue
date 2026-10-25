<script setup lang="ts">
/**
 * 总览：5 秒内回答「保护是否生效、有没有待处理的问题」。
 *
 * 所有数字都来自后端配置，界面不做任何推断：Hook 状态不等于 Healthy 就计入
 * 「需要处理」，无法判断（Unknown）也按需要处理展示。
 */
import {
  FolderOpened,
  Lock,
  Ticket,
  Warning,
} from "@element-plus/icons-vue";
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { PushConfirmation, Settings } from "../api";
import CardGrid from "../components/ui/CardGrid.vue";
import EmptyPanel from "../components/ui/EmptyPanel.vue";
import NoticeBanner from "../components/ui/NoticeBanner.vue";
import PageHeader from "../components/ui/PageHeader.vue";
import RepoRow from "../components/ui/RepoRow.vue";
import StatCard from "../components/ui/StatCard.vue";
import StatusTag from "../components/ui/StatusTag.vue";
import SurfaceCard from "../components/ui/SurfaceCard.vue";
import type { AppView } from "../navigation";
import type { StatusTone } from "../statusCodes";

const props = defineProps<{
  settings: Settings | null;
  pending: PushConfirmation | null;
  desktopAvailable: boolean;
}>();

const emit = defineEmits<{ navigate: [view: AppView] }>();

const { t } = useI18n();

/** `settings === null` 表示后端设置根本没取到：此时一切数字与状态都是「未知」。 */
const settingsReady = computed(() => props.settings !== null);
const repositories = computed(() => props.settings?.repositories ?? []);
const protectedCount = computed(
  () =>
    repositories.value.filter((repository) => repository.pushConfirmationEnabled)
      .length,
);
const unregisteredProtected = computed(
  () =>
    props.settings?.global.requireConfirmationForUnregisteredRepositories ??
    false,
);
// 未知 Hook 状态也算「需要处理」：不能把无法判断的情况显示成正常。
const hookIssues = computed(() =>
  repositories.value.filter((repository) => repository.hookStatus !== "Healthy"),
);
const previewRepositories = computed(() => repositories.value.slice(0, 3));
const pendingCount = computed(() => (props.pending === null ? 0 : 1));

/** 健康项：取不到设置时显示「状态未知」，而不是「正常」。 */
interface HealthItem {
  text: string;
  badge: string;
  tone: StatusTone;
}

function healthItem(
  error: string | null | undefined,
  healthyText: string,
  errorText: string,
  unknownText: string,
): HealthItem {
  if (!settingsReady.value) {
    return { text: unknownText, badge: t("common.unknown"), tone: "warning" };
  }
  return error === null || error === undefined
    ? { text: healthyText, badge: t("common.ok"), tone: "success" }
    : { text: errorText, badge: t("common.error"), tone: "danger" };
}

const configHealth = computed(() =>
  healthItem(
    props.settings?.configError,
    t("dashboard.configHealthy"),
    t("dashboard.configError"),
    t("dashboard.configUnknown"),
  ),
);
const ipcHealth = computed(() =>
  healthItem(
    props.settings?.ipcError,
    t("dashboard.ipcHealthy"),
    t("dashboard.ipcError"),
    t("dashboard.ipcUnknown"),
  ),
);
const hasAttention = computed(
  () =>
    !settingsReady.value ||
    configHealth.value.tone === "danger" ||
    ipcHealth.value.tone === "danger" ||
    hookIssues.value.length > 0,
);
/** 至少有一个仓库或未登记仓库受保护，才谈得上「保护运行中」。 */
const protectionActive = computed(
  () =>
    unregisteredProtected.value ||
    repositories.value.some((repository) => repository.pushConfirmationEnabled),
);

const statusTone = computed<"readonly" | "attention" | "running" | "idle">(
  () => {
    if (!props.desktopAvailable) {
      return "readonly";
    }
    if (hasAttention.value) {
      return "attention";
    }
    return protectionActive.value ? "running" : "idle";
  },
);

const STATUS_TEXT_KEYS = {
  readonly: "dashboard.statusReadOnly",
  attention: "dashboard.statusAttention",
  running: "dashboard.statusRunning",
  idle: "dashboard.statusIdle",
} as const;

const STATUS_TAG_TONES: Record<
  "readonly" | "attention" | "running" | "idle",
  StatusTone
> = {
  readonly: "info",
  attention: "warning",
  running: "success",
  idle: "info",
};

const statusText = computed(() => t(STATUS_TEXT_KEYS[statusTone.value]));
const statusTagTone = computed(() => STATUS_TAG_TONES[statusTone.value]);

const hookSummary = computed(() => {
  if (!settingsReady.value) {
    return { text: t("dashboard.hooksUnknown"), tone: "warning" as StatusTone };
  }
  if (repositories.value.length === 0) {
    return { text: t("dashboard.hooksEmpty"), tone: "info" as StatusTone };
  }
  if (hookIssues.value.length === 0) {
    return { text: t("dashboard.hooksHealthy"), tone: "success" as StatusTone };
  }
  return {
    text: t("dashboard.hooksIssues", { count: hookIssues.value.length }),
    tone: "warning" as StatusTone,
  };
});

function go(view: AppView): void {
  emit("navigate", view);
}
</script>

<template>
  <section aria-labelledby="dashboard-title">
    <PageHeader
      title-id="dashboard-title"
      :title="t('dashboard.title')"
      :subtitle="t('dashboard.subtitle')"
    >
      <template #actions>
        <StatusTag
          :label="statusText"
          :tone="statusTagTone"
        />
      </template>
    </PageHeader>

    <div class="grid grid-cols-[repeat(auto-fit,minmax(12rem,1fr))] gap-3">
      <StatCard
        :label="t('dashboard.stats.repositories')"
        :value="settingsReady ? repositories.length : '—'"
        :icon="FolderOpened"
        tone="brand"
      />
      <StatCard
        :label="t('dashboard.stats.protectedRepositories')"
        :value="settingsReady ? protectedCount : '—'"
        :icon="Lock"
        tone="success"
      />
      <StatCard
        :label="t('dashboard.stats.pending')"
        :value="pendingCount"
        :icon="Ticket"
        :hint="pendingCount > 0 ? t('dashboard.pendingOne') : t('dashboard.pendingNone')"
        :tone="pendingCount > 0 ? 'warning' : 'default'"
      />
      <StatCard
        :label="t('dashboard.stats.unregistered')"
        :value="settingsReady
          ? unregisteredProtected
            ? t('common.enabled')
            : t('common.disabled')
          : '—'
        "
        :icon="Warning"
        :tone="unregisteredProtected ? 'success' : 'default'"
      />
    </div>

    <NoticeBanner
      v-if="!desktopAvailable"
      type="info"
      :description="t('dashboard.readOnlyHint')"
      class="mt-3"
    />

    <CardGrid class="mt-3">
      <SurfaceCard :title="t('dashboard.healthTitle')">
        <ul class="m-0 grid list-none gap-2 p-0">
          <li class="flex items-center justify-between gap-2">
            <span class="text-meta text-ink">{{ configHealth.text }}</span>
            <StatusTag
              :label="configHealth.badge"
              :tone="configHealth.tone"
              :dot="false"
            />
          </li>
          <li class="flex items-center justify-between gap-2">
            <span class="text-meta text-ink">{{ ipcHealth.text }}</span>
            <StatusTag
              :label="ipcHealth.badge"
              :tone="ipcHealth.tone"
              :dot="false"
            />
          </li>
        </ul>

        <div class="mt-1 border-t border-line-soft pt-3">
          <h3 class="m-0 mb-2 text-meta font-semibold text-ink">
            {{ t("dashboard.hooksTitle") }}
          </h3>
          <StatusTag
            :label="hookSummary.text"
            :tone="hookSummary.tone"
            :dot="false"
          />
        </div>
      </SurfaceCard>

      <SurfaceCard :title="t('dashboard.repositoriesTitle')">
        <ul
          v-if="previewRepositories.length > 0"
          class="m-0 list-none p-0"
        >
          <RepoRow
            v-for="repository in previewRepositories"
            :key="repository.repositoryId"
            :repository="repository"
          />
        </ul>
        <EmptyPanel
          v-else-if="!settingsReady"
          :title="t('common.loadFailed')"
        />
        <EmptyPanel
          v-else
          :title="t('dashboard.hooksEmpty')"
        />
        <template #actions>
          <el-button
            size="small"
            @click="go('repositories')"
          >
            {{ t("common.manage") }}
          </el-button>
        </template>
      </SurfaceCard>

      <SurfaceCard :title="t('dashboard.quickTitle')">
        <div class="flex flex-wrap gap-2">
          <el-button
            type="primary"
            size="small"
            @click="go('repositories')"
          >
            {{ t("dashboard.addRepository") }}
          </el-button>
          <el-button
            size="small"
            @click="go('policies')"
          >
            {{ t("dashboard.reviewPolicies") }}
          </el-button>
          <el-button
            size="small"
            @click="go('approval')"
          >
            {{ t("dashboard.reviewApproval") }}
          </el-button>
        </div>
        <p class="m-0 text-meta leading-relaxed text-ink-muted">
          {{
            pendingCount > 0 ? t("dashboard.pendingOne") : t("dashboard.pendingNone")
          }}
        </p>
      </SurfaceCard>
    </CardGrid>
  </section>
</template>
