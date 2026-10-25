<script setup lang="ts">
import { CircleCheck } from "@element-plus/icons-vue";
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { PushConfirmation } from "../api";
import PushConfirmationPanel from "../components/PushConfirmation.vue";
import CardGrid from "../components/ui/CardGrid.vue";
import EmptyPanel from "../components/ui/EmptyPanel.vue";
import NoticeBanner from "../components/ui/NoticeBanner.vue";
import PageHeader from "../components/ui/PageHeader.vue";
import StatusTag from "../components/ui/StatusTag.vue";
import StepList from "../components/ui/StepList.vue";
import SurfaceCard from "../components/ui/SurfaceCard.vue";
import type { AppView } from "../navigation";

defineProps<{
  pending: PushConfirmation | null;
  busy: boolean;
  desktopAvailable: boolean;
}>();

const emit = defineEmits<{
  confirm: [];
  cancel: [];
  navigate: [view: AppView];
}>();

const { t } = useI18n();

const flowSteps = computed(() => [
  t("approval.flowStep1"),
  t("approval.flowStep2"),
  t("approval.flowStep3"),
]);
</script>

<template>
  <section aria-labelledby="approval-title">
    <PageHeader
      title-id="approval-title"
      :title="t('approval.title')"
      :subtitle="t('approval.subtitle')"
    >
      <template #actions>
        <StatusTag
          v-if="pending"
          :label="t('nav.pending')"
          tone="warning"
        />
      </template>
    </PageHeader>

    <NoticeBanner
      v-if="!desktopAvailable"
      type="info"
      :description="t('approval.readOnly')"
      class="mb-4"
    />

    <PushConfirmationPanel
      v-if="pending"
      :key="pending.requestId"
      :confirmation="pending"
      :busy="busy"
      @confirm="emit('confirm')"
      @cancel="emit('cancel')"
    />

    <CardGrid
      v-else
      min="24rem"
    >
      <EmptyPanel
        :title="t('approval.emptyTitle')"
        :description="t('approval.emptyBody')"
      >
        <template #icon>
          <CircleCheck />
        </template>
      </EmptyPanel>

      <SurfaceCard :title="t('approval.flowTitle')">
        <StepList :steps="flowSteps" />
        <NoticeBanner
          type="info"
          :description="t('approval.flowTerminal')"
        />
        <div class="flex flex-wrap gap-2">
          <el-button
            type="primary"
            plain
            @click="emit('navigate', 'policies')"
          >
            {{ t("dashboard.reviewPolicies") }}
          </el-button>
          <el-button @click="emit('navigate', 'repositories')">
            {{ t("policies.goToRepositories") }}
          </el-button>
        </div>
      </SurfaceCard>
    </CardGrid>
  </section>
</template>
