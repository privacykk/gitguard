<script setup lang="ts">
/**
 * 指标卡：总览页四个关键数字共用同一个外观。
 *
 * `hint` 用于补充单位或说明（例如「0 / 1」），`tone` 只影响强调色。
 */
import type { Component } from "vue";

defineProps<{
    label: string;
    value: string | number;
    hint?: string;
    /** 直接传入图标组件，避免依赖运行时全局注册。 */
    icon?: Component;
    tone?: "default" | "brand" | "success" | "warning";
}>();

const TONE_CLASSES: Record<string, string> = {
    default: "text-ink",
    brand: "text-brand-600",
    success: "text-success-600",
    warning: "text-warning-600",
};
</script>

<template>
  <div
    data-stat
    class="flex min-w-0 items-start justify-between gap-3 rounded-card border border-line bg-surface p-4 shadow-card"
  >
    <div class="min-w-0">
      <p class="m-0 text-meta leading-tight text-ink-muted">
        {{ label }}
      </p>
      <p
        :class="[
          'mt-2 text-2xl leading-none font-semibold tabular-nums',
          TONE_CLASSES[tone ?? 'default'],
        ]"
      >
        {{ value }}
      </p>
      <p
        v-if="hint"
        class="mt-1.5 text-caption text-ink-subtle"
      >
        {{ hint }}
      </p>
    </div>
    <el-icon
      v-if="icon"
      class="mt-0.5 text-lg text-ink-subtle"
      aria-hidden="true"
    >
      <component :is="icon" />
    </el-icon>
  </div>
</template>
