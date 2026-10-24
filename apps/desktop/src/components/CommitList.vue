<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import type { CommitInfo } from "../api";

const props = withDefaults(
    defineProps<{ commits: CommitInfo[]; initialLimit?: number }>(),
    { initialLimit: 5 },
);

const { t } = useI18n();

// 长列表默认折叠：确认界面要让人一眼看完，而不是滚动几百行。
const expanded = ref(false);
const visibleCommits = computed(() =>
    expanded.value ? props.commits : props.commits.slice(0, props.initialLimit),
);
const hiddenCount = computed(() => props.commits.length - props.initialLimit);
</script>

<template>
  <div class="commits">
    <p
      v-if="commits.length === 0"
      class="commits__empty"
    >
      {{ t("commit.empty") }}
    </p>
    <template v-else>
      <ul class="commits__list">
        <li
          v-for="commit in visibleCommits"
          :key="commit.sha"
          class="commits__item"
        >
          <code class="commits__sha">{{ commit.shortSha }}</code>
          <span class="commits__subject">{{ commit.subject }}</span>
          <span class="commits__meta">
            {{ t("commit.author") }}: {{ commit.author }} · {{ t("commit.time") }}:
            {{ commit.timestamp }}
          </span>
        </li>
      </ul>
      <button
        v-if="hiddenCount > 0"
        type="button"
        class="commits__toggle"
        @click="expanded = !expanded"
      >
        {{
          expanded
            ? t("commit.collapse")
            : t("commit.expand", { count: commits.length })
        }}
      </button>
    </template>
  </div>
</template>

<style scoped>
.commits__list {
    margin: 0;
    padding: 0;
    list-style: none;
}

.commits__item {
    display: grid;
    gap: 0.25rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid #e6ebf2;
}

.commits__sha {
    font-family: ui-monospace, SFMono-Regular, monospace;
    color: #3b5ccc;
}

.commits__subject {
    font-weight: 600;
}

.commits__meta,
.commits__empty {
    color: #55627a;
    font-size: 0.8125rem;
}

.commits__toggle {
    margin-top: 0.5rem;
    padding: 0.25rem 0.5rem;
    border: 1px solid #d7dee8;
    border-radius: 0.375rem;
    background: #ffffff;
    cursor: pointer;
}
</style>
