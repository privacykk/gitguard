<script setup lang="ts">
/**
 * 空态：标题 + 说明 + 可选动作。
 *
 * 用于「当前没有待确认推送」「还没有登记仓库」这类需要解释边界的场景，
 * 因此保留标题与正文两行，而不是只用一句 description。
 */
defineProps<{ title: string; description?: string }>();
</script>

<template>
  <div
    class="grid place-items-center gap-2 rounded-lg border border-dashed border-line bg-canvas/50 px-4 py-8 text-center"
  >
    <el-icon
      v-if="$slots.icon"
      class="text-xl text-ink-subtle"
      aria-hidden="true"
    >
      <slot name="icon" />
    </el-icon>
    <p class="m-0 text-sm font-medium text-ink">
      {{ title }}
    </p>
    <p
      v-if="description"
      class="m-0 max-w-xl text-meta leading-relaxed text-ink-muted"
    >
      {{ description }}
    </p>
    <div
      v-if="$slots.default"
      class="mt-1 flex flex-wrap items-center justify-center gap-2"
    >
      <slot />
    </div>
  </div>
</template>
