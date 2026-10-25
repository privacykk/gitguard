<script setup lang="ts">
/**
 * 有序步骤说明：确认流程、判定顺序共用。
 *
 * 用 `ol` + 序号圆点而不是 Element Plus 的步骤条：内容是静态说明，
 * 不需要「当前步骤」状态，语义化列表更贴近实际含义。
 */
defineProps<{ steps: ReadonlyArray<string> }>();
</script>

<template>
  <ol class="m-0 grid list-none gap-2 p-0">
    <li
      v-for="(step, index) in steps"
      :key="step"
      class="flex gap-2.5 text-meta leading-relaxed text-ink-muted"
    >
      <span
        class="mt-px grid size-5 shrink-0 place-items-center rounded-full bg-brand-50 text-micro font-semibold text-brand-600"
        aria-hidden="true"
      >
        {{ index + 1 }}
      </span>
      <span>{{ step }}</span>
    </li>
  </ol>
</template>
