<script setup lang="ts">
import { computed, onUnmounted, ref } from "vue";
import { Right } from "@element-plus/icons-vue";
import { useI18n } from "vue-i18n";
import type { PushConfirmation, RefPreview } from "../api";
import CommitList from "./CommitList.vue";
import SurfaceCard from "./ui/SurfaceCard.vue";

const props = defineProps<{ confirmation: PushConfirmation; busy: boolean }>();

const emit = defineEmits<{ confirm: []; cancel: [] }>();

const { t } = useI18n();

/** 变更类型只影响标签颜色；真正推什么由 CLI 与 Hook 决定。 */
const KIND_TAG_TYPES: Record<
  RefPreview["kind"],
  "primary" | "success" | "danger"
> = {
  create: "success",
  update: "primary",
  // 非快进更新可能丢弃远端提交，和删除一样用警示色。
  rewrite: "danger",
  delete: "danger",
};

// 用户必须亲手输入 yes：防止「误点确认按钮」直接把代码推到远端。
const answer = ref("");
const typedYes = computed(() => answer.value.trim() === "yes");

// 倒计时每秒刷新一次；到 0 只做展示，真正超时由后端与 Hook 的计时器决定。
const now = ref(Date.now());
const timer = window.setInterval(() => {
  now.value = Date.now();
}, 1000);
onUnmounted(() => window.clearInterval(timer));

const deadlineMs = computed(
  () =>
    (props.confirmation.receivedAtEpochSecs + props.confirmation.timeoutSeconds) *
    1000,
);
const remainingSeconds = computed(() =>
  Math.max(0, Math.ceil((deadlineMs.value - now.value) / 1000)),
);
const expired = computed(() => remainingSeconds.value === 0);
const canConfirm = computed(() => typedYes.value && !props.busy && !expired.value);

/** 进度条只做视觉提示，不参与「是否超时」的判断。 */
const remainingPercent = computed(() => {
  const total = props.confirmation.timeoutSeconds;
  if (total <= 0) {
    return 0;
  }
  return Math.min(100, Math.round((remainingSeconds.value / total) * 100));
});

/** 删除远端分支时本地 ref 是 `(delete)`，界面上不展示这个占位值。 */
function sourceRefLabel(reference: RefPreview): string {
  return reference.kind === "delete" ? "—" : reference.localRef;
}
</script>

<template>
  <SurfaceCard title-id="push-confirmation-title" :title="t('push.title')" :description="t('push.intro')" roomy>
    <el-descriptions :column="1" border size="small" label-width="7rem">
      <el-descriptions-item :label="t('push.repository')">
        <span class="break-all font-mono text-meta">
          {{ confirmation.repositoryPath }}
        </span>
      </el-descriptions-item>
      <el-descriptions-item :label="t('push.remote')">
        <span class="font-mono text-meta">{{ confirmation.remoteName }}</span>
      </el-descriptions-item>
      <el-descriptions-item :label="t('push.remoteUrl')">
        <span class="break-all font-mono text-meta">{{ confirmation.remoteUrl }}</span>
      </el-descriptions-item>
    </el-descriptions>

    <ul class="m-0 grid list-none gap-3 p-0">
      <li v-for="reference in confirmation.refs" :key="`${reference.localRef}->${reference.remoteRef}`"
        class="rounded-lg border border-line-soft bg-canvas/60 p-4">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <p class="m-0 flex flex-wrap items-center gap-2 font-mono text-meta">
            <span class="text-ink-subtle">{{ t("push.sourceRef") }}</span>
            <span class="rounded bg-surface px-1.5 py-0.5 text-ink">{{ sourceRefLabel(reference) }}</span>
            <el-icon class="text-ink-subtle" aria-hidden="true">
              <Right />
            </el-icon>
            <span class="text-ink-subtle">{{ t("push.targetRef") }}</span>
            <span class="rounded bg-surface px-1.5 py-0.5 text-ink">{{ reference.remoteRef }}</span>
          </p>
          <el-tag :type="KIND_TAG_TYPES[reference.kind]" size="small" effect="light" round disable-transitions>
            {{ t(`push.kind.${reference.kind}`) }}
          </el-tag>
        </div>

        <h3 class="mt-3 mb-2 text-meta font-semibold text-ink">
          {{ t("push.commits") }}
        </h3>
        <p v-if="reference.commits.length === 0" class="m-0 text-meta text-ink-muted">
          {{ t("push.noCommits") }}
        </p>
        <CommitList v-else :commits="reference.commits" />
      </li>
    </ul>

    <div class="grid gap-1.5" aria-live="polite">
      <p class="m-0 text-meta font-medium" :class="expired ? 'text-danger-600' : 'text-ink'">
        {{
          expired
            ? t("push.expired")
            : t("push.remaining", { seconds: remainingSeconds })
        }}
      </p>
      <el-progress :percentage="remainingPercent" :status="expired ? 'exception' : undefined" :stroke-width="6"
        :show-text="false" />
    </div>

    <form data-confirm-form class="grid gap-3 border-t border-line-soft pt-4"
      @submit.prevent="canConfirm && emit('confirm')">
      <label class="text-sm font-medium text-ink" for="confirmation-answer">
        {{ t("push.yesLabel") }}
      </label>
      <el-input id="confirmation-answer" v-model="answer" type="text" autocomplete="off" class="max-w-60"
        :placeholder="t('push.inputPlaceholder')" :disabled="busy || expired" />
      <div class="flex flex-wrap gap-2">
        <el-button type="primary" native-type="submit" data-confirm :disabled="!canConfirm">
          {{ busy ? t("push.executing") : t("push.confirmButton") }}
        </el-button>
        <el-button data-cancel :disabled="busy" @click="emit('cancel')">
          {{ t("push.cancelButton") }}
        </el-button>
      </div>
    </form>
  </SurfaceCard>
</template>
