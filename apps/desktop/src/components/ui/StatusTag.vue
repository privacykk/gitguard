<script setup lang="ts">
/**
 * 状态标签：把稳定的状态码映射成统一的颜色与胶囊外观。
 *
 * 只负责展示，颜色含义固定——绿=正常、黄=需注意、红=异常、灰=中性。
 */
import { computed } from "vue";
import type { StatusTone } from "../../statusCodes";

const props = withDefaults(
    defineProps<{ label: string; tone?: StatusTone; dot?: boolean }>(),
    { tone: "info", dot: true },
);

const TAG_TYPES: Record<StatusTone, "success" | "warning" | "danger" | "info"> = {
    success: "success",
    warning: "warning",
    danger: "danger",
    info: "info",
};

const tagType = computed(() => TAG_TYPES[props.tone]);
</script>

<template>
  <el-tag
    :type="tagType"
    effect="light"
    round
    size="small"
    disable-transitions
    class="!font-medium"
  >
    <span class="inline-flex items-center gap-1.5">
      <span
        v-if="dot"
        class="size-1.5 rounded-full bg-current"
        aria-hidden="true"
      />
      {{ label }}
    </span>
  </el-tag>
</template>
