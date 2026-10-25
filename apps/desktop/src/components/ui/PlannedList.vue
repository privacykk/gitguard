<script setup lang="ts">
/**
 * 规划中能力的清单：虚线小卡 + 标题 + 一句说明。
 *
 * 身份管理、安全策略、操作日志三页都在列「规划中的能力」，
 * 统一走这里，保证虚线、间距和字号一致。
 */
defineProps<{ items: ReadonlyArray<{ title: string; body: string }> }>();
</script>

<template>
  <ul class="m-0 grid list-none gap-2 p-0">
    <li
      v-for="item in items"
      :key="item.title"
      class="rounded-lg border border-dashed border-line bg-surface/70 px-3 py-2.5"
    >
      <p class="m-0 text-meta font-semibold text-ink">
        {{ item.title }}
      </p>
      <p class="mt-1 text-meta leading-relaxed text-ink-muted">
        {{ item.body }}
      </p>
    </li>
  </ul>
</template>
