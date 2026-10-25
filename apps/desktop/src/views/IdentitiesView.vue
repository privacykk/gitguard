<script setup lang="ts">
/**
 * 身份管理（规划中）。
 *
 * 这一页的价值是把安全边界说清楚：GitGuard 不管理 Git 身份，
 * 也不使用 `user.name` / `user.email` 作为远端认证身份。
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import CardGrid from "../components/ui/CardGrid.vue";
import PageHeader from "../components/ui/PageHeader.vue";
import PlannedList from "../components/ui/PlannedList.vue";
import StatusTag from "../components/ui/StatusTag.vue";
import SurfaceCard from "../components/ui/SurfaceCard.vue";

const { t } = useI18n();

const plannedItems = computed(() => [
  {
    title: t("identities.plannedCreate"),
    body: t("identities.plannedCreateBody"),
  },
  {
    title: t("identities.plannedBind"),
    body: t("identities.plannedBindBody"),
  },
  {
    title: t("identities.plannedPriority"),
    body: t("identities.plannedPriorityBody"),
  },
]);
</script>

<template>
  <section aria-labelledby="identities-title">
    <PageHeader
      title-id="identities-title"
      :title="t('identities.title')"
      :subtitle="t('identities.subtitle')"
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
      <SurfaceCard :title="t('identities.unavailableTitle')">
        <p class="m-0 text-meta leading-relaxed text-ink-muted">
          {{ t("identities.unavailableBody") }}
        </p>
      </SurfaceCard>

      <SurfaceCard
        :title="t('identities.boundaryTitle')"
        tone="brand"
      >
        <p class="m-0 text-meta leading-relaxed text-ink">
          {{ t("identities.boundaryBody") }}
        </p>
      </SurfaceCard>

      <SurfaceCard
        :title="t('identities.plannedTitle')"
        tone="planned"
      >
        <PlannedList :items="plannedItems" />
      </SurfaceCard>
    </CardGrid>
  </section>
</template>
