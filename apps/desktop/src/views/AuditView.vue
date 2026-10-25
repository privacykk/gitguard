<script setup lang="ts">
/**
 * 操作日志（规划中）。
 *
 * 明确说明当前只在 stderr 输出错误、不持久化审计；
 * 页面上的任何内容都不构成审计凭证。
 */
import { Document } from "@element-plus/icons-vue";
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import CardGrid from "../components/ui/CardGrid.vue";
import NoticeBanner from "../components/ui/NoticeBanner.vue";
import PageHeader from "../components/ui/PageHeader.vue";
import PlannedList from "../components/ui/PlannedList.vue";
import StatusTag from "../components/ui/StatusTag.vue";
import SurfaceCard from "../components/ui/SurfaceCard.vue";

const { t } = useI18n();

const plannedItems = computed(() => [
  {
    title: t("audit.plannedPush"),
    body: t("audit.plannedPushBody"),
  },
  {
    title: t("audit.plannedConfig"),
    body: t("audit.plannedConfigBody"),
  },
  {
    title: t("audit.plannedExport"),
    body: t("audit.plannedExportBody"),
  },
]);
</script>

<template>
  <section aria-labelledby="audit-title">
    <PageHeader
      title-id="audit-title"
      :title="t('audit.title')"
      :subtitle="t('audit.subtitle')"
    >
      <template #actions>
        <StatusTag
          :label="t('common.planned')"
          tone="info"
          :dot="false"
        />
      </template>
    </PageHeader>

    <CardGrid>
      <SurfaceCard :title="t('audit.unavailableTitle')">
        <p class="m-0 text-meta leading-relaxed text-ink-muted">
          {{ t("audit.unavailableBody") }}
        </p>
        <template #badge>
          <el-icon class="text-ink-subtle">
            <Document />
          </el-icon>
        </template>
      </SurfaceCard>

      <SurfaceCard
        :title="t('audit.plannedTitle')"
        tone="planned"
      >
        <PlannedList :items="plannedItems" />
      </SurfaceCard>
    </CardGrid>

    <NoticeBanner
      type="warning"
      :description="t('audit.boundary')"
      class="mt-3"
    />
  </section>
</template>
