<script setup lang="ts">
/**
 * 卡片栅格：宽屏自动加列，窄窗回落单列。
 *
 * 七个页面的卡片都放进来，保证全屏时内容铺满且在剩余空间里居中，
 * 不再出现「左边一条、右边一片空白」的排版。
 */
withDefaults(defineProps<{ min?: string }>(), { min: "22rem" });
</script>

<template>
  <div
    class="grid items-start gap-3"
    :style="{ gridTemplateColumns: `repeat(auto-fit, minmax(min(100%, ${min}), 1fr))` }"
  >
    <slot />
  </div>
</template>
