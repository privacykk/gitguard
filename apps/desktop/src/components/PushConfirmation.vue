<script setup lang="ts">
import { computed, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import type { PushConfirmation, RefPreview } from "../api";
import CommitList from "./CommitList.vue";

const props = defineProps<{ confirmation: PushConfirmation; busy: boolean }>();

const emit = defineEmits<{ confirm: []; cancel: [] }>();

const { t } = useI18n();

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

/** 删除远端分支时本地 ref 是 `(delete)`，界面上不展示这个占位值。 */
function sourceRefLabel(reference: RefPreview): string {
    return reference.kind === "delete" ? "—" : reference.localRef;
}
</script>

<template>
  <section
    class="confirmation"
    aria-labelledby="push-confirmation-title"
  >
    <h2 id="push-confirmation-title">
      {{ t("push.title") }}
    </h2>
    <p class="confirmation__intro">
      {{ t("push.intro") }}
    </p>

    <dl class="confirmation__meta">
      <dt>{{ t("push.repository") }}</dt>
      <dd>{{ confirmation.repositoryPath }}</dd>
      <dt>{{ t("push.remote") }}</dt>
      <dd>{{ confirmation.remoteName }}</dd>
      <dt>{{ t("push.remoteUrl") }}</dt>
      <dd>{{ confirmation.remoteUrl }}</dd>
    </dl>

    <ul class="confirmation__refs">
      <li
        v-for="reference in confirmation.refs"
        :key="`${reference.localRef}->${reference.remoteRef}`"
        class="confirmation__ref"
      >
        <p class="confirmation__ref-target">
          <span>{{ t("push.sourceRef") }}: {{ sourceRefLabel(reference) }}</span>
          <span aria-hidden="true">→</span>
          <span>{{ t("push.targetRef") }}: {{ reference.remoteRef }}</span>
        </p>
        <p
          class="confirmation__kind"
          :class="`confirmation__kind--${reference.kind}`"
        >
          {{ t(`push.kind.${reference.kind}`) }}
        </p>
        <h3 class="confirmation__commits-title">
          {{ t("push.commits") }}
        </h3>
        <p
          v-if="reference.commits.length === 0"
          class="confirmation__no-commits"
        >
          {{ t("push.noCommits") }}
        </p>
        <CommitList
          v-else
          :commits="reference.commits"
        />
      </li>
    </ul>

    <p
      class="confirmation__timer"
      :class="{ 'confirmation__timer--expired': expired }"
      aria-live="polite"
    >
      {{
        expired
          ? t("push.expired")
          : t("push.remaining", { seconds: remainingSeconds })
      }}
    </p>

    <form
      class="confirmation__actions"
      @submit.prevent="canConfirm && emit('confirm')"
    >
      <label
        class="confirmation__label"
        for="confirmation-answer"
      >
        {{ t("push.yesLabel") }}
      </label>
      <input
        id="confirmation-answer"
        v-model="answer"
        class="confirmation__input"
        type="text"
        autocomplete="off"
        :placeholder="t('push.inputPlaceholder')"
        :disabled="busy || expired"
      >
      <div class="confirmation__buttons">
        <button
          type="submit"
          class="confirmation__confirm"
          :disabled="!canConfirm"
        >
          {{ busy ? t("push.executing") : t("push.confirmButton") }}
        </button>
        <button
          type="button"
          class="confirmation__cancel"
          :disabled="busy"
          @click="emit('cancel')"
        >
          {{ t("push.cancelButton") }}
        </button>
      </div>
    </form>
  </section>
</template>

<style scoped>
.confirmation {
    padding: 1.5rem;
    border: 1px solid #d7dee8;
    border-radius: 0.75rem;
    background: #ffffff;
}

.confirmation__intro {
    color: #55627a;
}

.confirmation__meta {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.25rem 1rem;
    margin: 1rem 0;
    font-size: 0.875rem;
}

.confirmation__meta dt {
    color: #55627a;
}

.confirmation__meta dd {
    margin: 0;
    overflow-wrap: anywhere;
}

.confirmation__refs {
    margin: 0;
    padding: 0;
    list-style: none;
}

.confirmation__ref {
    padding: 1rem 0;
    border-top: 1px solid #e6ebf2;
}

.confirmation__ref-target {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 0.875rem;
}

.confirmation__kind {
    display: inline-block;
    margin: 0.5rem 0;
    padding: 0.125rem 0.5rem;
    border-radius: 999px;
    font-size: 0.8125rem;
    background: #eef2ff;
    color: #3b5ccc;
}

.confirmation__kind--create {
    background: #eaf7ef;
    color: #1c6b3a;
}

.confirmation__kind--delete {
    background: #fdecec;
    color: #a12222;
}

.confirmation__commits-title {
    margin: 0.5rem 0 0.25rem;
    font-size: 0.9375rem;
}

.confirmation__no-commits {
    margin: 0;
    color: #55627a;
    font-size: 0.875rem;
}

.confirmation__timer {
    margin: 1rem 0 0;
    font-weight: 600;
}

.confirmation__timer--expired {
    color: #a12222;
}

.confirmation__actions {
    display: grid;
    gap: 0.5rem;
    margin-top: 1rem;
}

.confirmation__label {
    font-size: 0.875rem;
}

.confirmation__input {
    padding: 0.5rem;
    border: 1px solid #d7dee8;
    border-radius: 0.375rem;
}

.confirmation__buttons {
    display: flex;
    gap: 0.5rem;
}

.confirmation__confirm {
    padding: 0.5rem 0.875rem;
    border: none;
    border-radius: 0.375rem;
    background: #3b5ccc;
    color: #ffffff;
    cursor: pointer;
}

.confirmation__confirm:disabled {
    background: #c3cbe0;
    cursor: not-allowed;
}

.confirmation__cancel {
    padding: 0.5rem 0.875rem;
    border: 1px solid #d7dee8;
    border-radius: 0.375rem;
    background: #ffffff;
    cursor: pointer;
}
</style>
