<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { toStatusMessageKey } from "../i18n/developmentStatus";

const { t } = useI18n();

const statusMessageKey = ref(toStatusMessageKey(undefined));

onMounted(async () => {
  try {
    statusMessageKey.value = toStatusMessageKey(
      await invoke("development_status"),
    );
  } catch {
    // 桌面容器不可用（例如在浏览器直接打开 Vite）时保留 unknown 文案，不推断推送结果。
    statusMessageKey.value = toStatusMessageKey(undefined);
  }
});
</script>

<template>
  <section
    aria-labelledby="status-title"
    class="status-card"
  >
    <h2 id="status-title">
      {{ t("development.title") }}
    </h2>
    <p>{{ t(statusMessageKey) }}</p>
  </section>
</template>

<style scoped>
.status-card {
  padding: 1.25rem;
  border: 1px solid #d7dee8;
  border-radius: 0.75rem;
  background: #f8fafc;
}
</style>
