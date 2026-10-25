<script setup lang="ts">
/**
 * 内容卡片：标题 + 说明 + 右上角动作，是七个页面里重复度最高的容器。
 *
 * `tone="planned"` 用于路线图里的未实现能力：虚线边框 + 「规划中」标签，
 * 让界面自己说清楚「这只是计划」，不靠文案补充说明。
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";

const props = withDefaults(
  defineProps<{
    title?: string;
    /** 标题元素的 id，供 `aria-labelledby` 引用。 */
    titleId?: string;
    description?: string;
    tone?: "plain" | "brand" | "planned";
    /** 卡片内间距放大，用于纯文本卡片。 */
    roomy?: boolean;
  }>(),
  { title: "", titleId: "", description: "", tone: "plain", roomy: false },
);

const { t } = useI18n();

const toneClasses = computed(() => {
  switch (props.tone) {
    case "brand":
      return "border-brand-100 bg-brand-50/70";
    case "planned":
      return "border-dashed border-line bg-canvas/60";
    default:
      return "border-line bg-surface shadow-card";
  }
});
</script>

<template>
  <section
    :class="[
      'flex min-w-0 flex-col rounded-card border p-5',
      toneClasses,
      roomy ? 'gap-4' : 'gap-3',
    ]"
  >
    <header
      v-if="title || $slots.actions"
      class="flex items-start justify-between gap-3"
    >
      <div class="min-w-0">
        <h2
          v-if="title"
          :id="titleId"
          class="m-0 flex flex-wrap items-center gap-2 text-title font-semibold text-ink"
        >
          {{ title }}
          <el-tag
            v-if="tone === 'planned'"
            type="info"
            size="small"
            effect="plain"
            round
            disable-transitions
          >
            {{ t("common.planned") }}
          </el-tag>
          <slot name="badge" />
        </h2>
        <p
          v-if="description"
          class="mt-1 text-meta leading-relaxed text-ink-muted"
        >
          {{ description }}
        </p>
      </div>
      <div
        v-if="$slots.actions"
        class="flex shrink-0 items-center gap-2"
      >
        <slot name="actions" />
      </div>
    </header>

    <slot />
  </section>
</template>
