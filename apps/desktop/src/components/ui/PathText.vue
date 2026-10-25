<script setup lang="ts">
/**
 * 路径文本：仓库路径可能很长，界面统一做单行截断 + 悬浮查看完整值。
 *
 * 不参与任何判断，只负责把后端给的原始路径原样展示出来。
 */
defineProps<{ path: string }>();
</script>

<template>
  <el-tooltip
    :content="path"
    placement="top-start"
    :show-after="400"
    :disabled="path.length === 0"
  >
    <code class="block max-w-full truncate font-mono text-meta text-ink">{{ path }}</code>
  </el-tooltip>
</template>
