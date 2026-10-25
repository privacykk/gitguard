<script setup lang="ts">
/**
 * 全局提示条：加载失败、确认成功、待确认横幅共用。
 *
 * `error` 用 `role="alert"` 立即播报，其余用 `role="status"`，
 * 避免把「已确认」这类信息读成打断式告警。
 */
import { computed } from "vue";

const props = defineProps<{
  type: "info" | "success" | "warning" | "error";
  title?: string;
  description?: string;
  /** 排查建议：只提示下一步该看什么，不改变任何状态。 */
  hint?: string;
}>();

const role = computed(() => (props.type === "error" ? "alert" : "status"));
</script>

<template>
  <el-alert
    :type="type"
    :title="title"
    :closable="false"
    show-icon
    :role="role"
    class="items-start! rounded-lg"
  >
    <template
      v-if="description || hint || $slots.action"
      #default
    >
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div class="min-w-0 space-y-1">
          <span
            v-if="description"
            class="block text-meta leading-relaxed"
          >
            {{ description }}
          </span>
          <p
            v-if="hint"
            class="m-0 text-caption leading-relaxed opacity-80"
          >
            {{ hint }}
          </p>
        </div>
        <slot name="action" />
      </div>
    </template>
  </el-alert>
</template>
