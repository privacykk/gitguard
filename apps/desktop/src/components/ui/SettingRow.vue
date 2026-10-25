<script setup lang="ts">
/**
 * 设置行：左侧「名称 + 说明」，右侧放开关/按钮等控件。
 *
 * 仓库管理、安全策略、设置三处都是这个结构，抽出来避免各自写一遍布局。
 */
defineProps<{ label: string; hint?: string }>();
</script>

<template>
  <div class="flex flex-wrap items-center justify-between gap-3">
    <div class="min-w-0">
      <p class="m-0 text-sm font-medium text-ink">
        {{ label }}
      </p>
      <p
        v-if="hint"
        class="mt-0.5 text-meta leading-relaxed text-ink-muted"
      >
        {{ hint }}
      </p>
    </div>
    <div class="flex shrink-0 items-center gap-2">
      <slot />
    </div>
  </div>
</template>
