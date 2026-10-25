<script setup lang="ts">
/**
 * 页面标题区。
 *
 * 每个页面都要「标题 + 一句说明 + 右侧状态/动作」，抽出来保证七个页面的
 * 间距与字号一致；标题 id 由调用方传入，供 `aria-labelledby` 复用。
 */
defineProps<{ titleId: string; title: string; subtitle?: string }>();
</script>

<template>
  <header class="mb-6 flex flex-wrap items-start justify-between gap-4">
    <div class="min-w-0">
      <h1
        :id="titleId"
        class="m-0 text-2xl leading-tight font-semibold tracking-tight text-ink"
      >
        {{ title }}
      </h1>
      <p
        v-if="subtitle"
        class="mt-1.5 max-w-3xl text-sm leading-relaxed text-ink-muted"
      >
        {{ subtitle }}
      </p>
    </div>
    <div
      v-if="$slots.actions"
      class="flex shrink-0 flex-wrap items-center gap-2"
    >
      <slot name="actions" />
    </div>
  </header>
</template>
