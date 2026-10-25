<script setup lang="ts">
/**
 * 操作日志：展示 CLI 写入本机的推送确认事件，并导出脱敏诊断摘要。
 *
 * 这一页只读：事件由 CLI 在推送时写入，桌面端不补记、不改写，
 * 避免同一次推送出现两条互相矛盾的记录。审计文件是本机普通文件，
 * 可被本机用户修改，因此界面上的内容不构成防篡改凭证。
 */
import { Document } from "@element-plus/icons-vue";
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import {
  buildDiagnostics,
  copyText,
  exportDiagnostics,
  getAuditLog,
  type AuditLog,
} from "../api";
import CardGrid from "../components/ui/CardGrid.vue";
import EmptyPanel from "../components/ui/EmptyPanel.vue";
import NoticeBanner from "../components/ui/NoticeBanner.vue";
import PageHeader from "../components/ui/PageHeader.vue";
import PathText from "../components/ui/PathText.vue";
import StatCard from "../components/ui/StatCard.vue";
import StatusTag from "../components/ui/StatusTag.vue";
import SurfaceCard from "../components/ui/SurfaceCard.vue";
import {
  commandErrorCode,
  toAuditChannelMessageKey,
  toAuditEventMessageKey,
  toAuditReasonMessageKey,
  toAuditResultMessageKey,
  toAuditResultTone,
  toCommandErrorHintKey,
  toCommandErrorMessageKey,
} from "../statusCodes";

const props = defineProps<{
  desktopAvailable: boolean;
  /** 最近一次命令失败的稳定错误码，用于让诊断摘要与用户看到的报错对上。 */
  lastErrorCode: string | null;
}>();

const { t } = useI18n();

const log = ref<AuditLog | null>(null);
const loading = ref(false);
/** 读取失败（命令失败或返回值形状不对）的文案 key。 */
const loadErrorKey = ref<string | null>(null);
/** 诊断摘要操作的忙碌状态，避免重复点击。 */
const busy = ref<"copy" | "export" | null>(null);

interface ActionError {
  key: string;
  hintKey: string | null;
}

interface ActionNotice {
  tone: "success" | "info";
  key: string;
  params: Record<string, string>;
}

const actionError = ref<ActionError | null>(null);
const notice = ref<ActionNotice | null>(null);

const events = computed(() => log.value?.events ?? []);

/** 读取审计页数据；不可用或失败时清空旧数据，不用上一次的内容顶替。 */
async function load(): Promise<void> {
  if (!props.desktopAvailable) {
    log.value = null;
    loadErrorKey.value = null;
    return;
  }

  loading.value = true;
  loadErrorKey.value = null;
  try {
    log.value = await getAuditLog();
  } catch (reason) {
    log.value = null;
    loadErrorKey.value = toCommandErrorMessageKey(commandErrorCode(reason));
  } finally {
    loading.value = false;
  }
}

/** 复制与导出共用的失败处理：只展示稳定错误码对应的文案与建议。 */
function fail(reason: unknown): void {
  const code = commandErrorCode(reason);
  actionError.value = {
    key: toCommandErrorMessageKey(code),
    hintKey: toCommandErrorHintKey(code),
  };
}

async function handleCopy(): Promise<void> {
  actionError.value = null;
  notice.value = null;
  busy.value = "copy";
  try {
    const text = await buildDiagnostics(props.lastErrorCode);
    if (await copyText(text)) {
      notice.value = { tone: "success", key: "audit.copied", params: {} };
    } else {
      // 剪贴板不可用不改变摘要内容，只是没地方粘贴：提示改用导出。
      actionError.value = { key: "audit.copyFailed", hintKey: null };
    }
  } catch (reason) {
    fail(reason);
  } finally {
    busy.value = null;
  }
}

async function handleExport(): Promise<void> {
  actionError.value = null;
  notice.value = null;
  busy.value = "export";
  try {
    const path = await exportDiagnostics(props.lastErrorCode);
    notice.value = {
      tone: "info",
      key: "audit.exported",
      params: { path },
    };
  } catch (reason) {
    fail(reason);
  } finally {
    busy.value = null;
  }
}

onMounted(load);
</script>

<template>
  <section aria-labelledby="audit-title">
    <PageHeader
      title-id="audit-title"
      :title="t('audit.title')"
      :subtitle="t('audit.subtitle')"
    >
      <template #actions>
        <el-button
          size="small"
          :disabled="!desktopAvailable"
          :loading="loading"
          data-audit-refresh
          @click="load"
        >
          {{ t("audit.refresh") }}
        </el-button>
        <el-button
          size="small"
          :disabled="!desktopAvailable"
          :loading="busy === 'copy'"
          data-audit-copy
          @click="handleCopy"
        >
          {{ t("audit.copyDiagnostics") }}
        </el-button>
        <el-button
          size="small"
          :disabled="!desktopAvailable"
          :loading="busy === 'export'"
          data-audit-export
          @click="handleExport"
        >
          {{ t("audit.exportDiagnostics") }}
        </el-button>
      </template>
    </PageHeader>

    <div class="mb-3 grid gap-2">
      <NoticeBanner
        v-if="!desktopAvailable"
        type="info"
        :description="t('audit.readOnly')"
      />
      <NoticeBanner
        v-if="loadErrorKey"
        type="error"
        :description="t(loadErrorKey)"
        :hint="t('audit.loadErrorHint')"
        data-audit-load-error
      />
      <NoticeBanner
        v-else-if="log?.loadError"
        type="error"
        :description="t('audit.loadErrorBody')"
        :hint="t(toCommandErrorHintKey(log.loadError))"
        data-audit-file-error
      />
      <NoticeBanner
        v-if="log?.configError"
        type="error"
        :description="t(toCommandErrorMessageKey(log.configError))"
        :hint="t(toCommandErrorHintKey(log.configError))"
      />
      <NoticeBanner
        v-if="log?.ipcError"
        type="warning"
        :description="t(toCommandErrorMessageKey(log.ipcError))"
        :hint="t(toCommandErrorHintKey(log.ipcError))"
      />
      <NoticeBanner
        v-if="log?.truncated"
        type="warning"
        :description="t('audit.truncated')"
      />
      <NoticeBanner
        v-if="actionError"
        type="error"
        :description="t(actionError.key)"
        :hint="actionError.hintKey ? t(actionError.hintKey) : undefined"
        data-audit-action-error
      />
      <NoticeBanner
        v-else-if="notice"
        :type="notice.tone"
        :description="t(notice.key, notice.params)"
        data-audit-action-notice
      />
    </div>

    <CardGrid
      v-if="log"
      min="15rem"
      class="mb-3"
    >
      <StatCard
        :label="t('audit.stats.events')"
        :value="events.length"
        :hint="t('audit.stats.eventsHint')"
      />
      <StatCard
        :label="t('audit.stats.skipped')"
        :value="log.skippedLines"
        :tone="log.skippedLines > 0 ? 'warning' : 'default'"
        :hint="t('audit.stats.skippedHint')"
      />
      <StatCard
        :label="t('audit.stats.retention')"
        :value="t('audit.stats.retentionValue', {
          days: log.retentionDays,
          entries: log.maxEntries,
        })
        "
        :hint="t('audit.stats.retentionHint')"
      />
    </CardGrid>

    <SurfaceCard
      :title="t('audit.eventsTitle')"
      :description="t('audit.eventsHint')"
    >
      <el-skeleton
        v-if="loading"
        :rows="4"
        animated
      />
      <EmptyPanel
        v-else-if="events.length === 0"
        :title="t('audit.emptyTitle')"
        :description="t('audit.emptyBody')"
      >
        <template #icon>
          <Document />
        </template>
      </EmptyPanel>
      <ul
        v-else
        class="m-0 flex list-none flex-col gap-2 p-0"
      >
        <li
          v-for="(event, index) in events"
          :key="`${event.timestampEpochSecs}-${index}`"
          data-audit-event
          class="rounded-lg border border-line bg-canvas/40 p-3"
        >
          <div class="flex flex-wrap items-center justify-between gap-2">
            <div class="flex min-w-0 flex-wrap items-center gap-2">
              <StatusTag
                :label="t(toAuditEventMessageKey(event.event))"
                :tone="toAuditResultTone(event.result)"
              />
              <StatusTag
                :label="t(toAuditResultMessageKey(event.result))"
                :tone="toAuditResultTone(event.result)"
                :dot="false"
              />
              <span class="text-meta text-ink-muted">
                {{ t(toAuditReasonMessageKey(event.reasonCode)) }}
              </span>
            </div>
            <span class="text-caption tabular-nums text-ink-subtle">
              {{ event.timestamp }}
            </span>
          </div>

          <dl class="mt-2 grid gap-x-4 gap-y-1 text-caption sm:grid-cols-2">
            <div class="flex min-w-0 gap-1.5">
              <dt class="shrink-0 text-ink-subtle">
                {{ t("audit.fields.channel") }}
              </dt>
              <dd class="m-0 min-w-0 truncate text-ink">
                {{ t(toAuditChannelMessageKey(event.channel)) }}
              </dd>
            </div>
            <div class="flex min-w-0 gap-1.5">
              <dt class="shrink-0 text-ink-subtle">
                {{ t("audit.fields.remote") }}
              </dt>
              <dd class="m-0 min-w-0 truncate text-ink">
                {{ event.remoteName }} · {{ event.remoteUrl }}
              </dd>
            </div>
            <div class="flex min-w-0 gap-1.5">
              <dt class="shrink-0 text-ink-subtle">
                {{ t("audit.fields.repository") }}
              </dt>
              <dd class="m-0 min-w-0">
                <PathText :path="event.repositoryPath" />
              </dd>
            </div>
            <div
              v-if="event.repositoryId"
              class="flex min-w-0 gap-1.5"
            >
              <dt class="shrink-0 text-ink-subtle">
                {{ t("audit.fields.repositoryId") }}
              </dt>
              <dd class="m-0 min-w-0 truncate font-mono text-ink">
                {{ event.repositoryId }}
              </dd>
            </div>
            <div class="flex min-w-0 gap-1.5">
              <dt class="shrink-0 text-ink-subtle">
                {{ t("audit.fields.request") }}
              </dt>
              <dd class="m-0 min-w-0 truncate font-mono text-ink">
                {{ event.requestId ?? t("common.none") }}
              </dd>
            </div>
            <div class="flex min-w-0 gap-1.5">
              <dt class="shrink-0 text-ink-subtle">
                {{ t("audit.fields.refs") }}
              </dt>
              <dd class="m-0 min-w-0 truncate tabular-nums text-ink">
                {{ event.refCount }}
              </dd>
            </div>
          </dl>
        </li>
      </ul>
    </SurfaceCard>

    <SurfaceCard
      v-if="log"
      :title="t('audit.pathTitle')"
      :description="t('audit.pathHint')"
      class="mt-3"
    >
      <PathText :path="log.path" />
    </SurfaceCard>

    <NoticeBanner
      type="warning"
      :description="t('audit.boundary')"
      class="mt-3"
    />
  </section>
</template>
