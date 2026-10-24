<script setup lang="ts">
import DevelopmentStatus from './components/DevelopmentStatus.vue'
</script>

<template>
  <main class="shell">
    <p class="eyebrow">
      本地优先的 Git 推送保护
    </p>
    <h1>GitGuard</h1>
    <p class="intro">
      第一周工程原型：核心策略与 Hook 退出行为由 Rust 后端负责。
    </p>
    <DevelopmentStatus />
  </main>
</template>
