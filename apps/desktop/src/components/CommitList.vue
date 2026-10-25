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
  <div class="grid gap-2">
    <p
      v-if="commits.length === 0"
      class="m-0 text-meta text-ink-muted"
    >
      {{ t("commit.empty") }}
    </p>
    <template v-else>
      <ul class="m-0 grid list-none gap-0 p-0">
        <li
          v-for="commit in visibleCommits"
          :key="commit.sha"
          class="grid gap-1 border-b border-line-soft py-2 last:border-b-0"
        >
          <div class="flex flex-wrap items-baseline gap-2">
            <code
              class="rounded bg-brand-50 px-1.5 py-0.5 font-mono text-caption text-brand-700"
            >{{ commit.shortSha }}</code>
            <span class="min-w-0 text-meta font-medium text-ink">{{ commit.subject }}</span>
          </div>
          <span class="text-caption text-ink-subtle">
            {{ t("commit.author") }}: {{ commit.author }} · {{ t("commit.time") }}:
            {{ commit.timestamp }}
          </span>
        </li>
      </ul>
      <el-button
        v-if="hiddenCount > 0"
        link
        type="primary"
        size="small"
        class="justify-self-start"
        @click="expanded = !expanded"
      >
        {{
          expanded
            ? t("commit.collapse")
            : t("commit.expand", { count: commits.length })
        }}
      </el-button>
    </template>
  </div>
</template>
