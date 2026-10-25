<script setup lang="ts">
/**
 * 仓库管理：登记本地仓库、安装 Hook、按仓库开关推送保护。
 *
 * 界面只发命令并展示后端返回的配置；Hook 是否正常由 Rust 判定，
 * 未知状态按 fail-closed 文案显示（见 `statusCodes`）。
 */
import { FolderOpened } from "@element-plus/icons-vue";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import {
  isDesktopAvailable,
  pickRepositoryDirectory,
  type Settings,
} from "../api";
import CardGrid from "../components/ui/CardGrid.vue";
import EmptyPanel from "../components/ui/EmptyPanel.vue";
import NoticeBanner from "../components/ui/NoticeBanner.vue";
import PageHeader from "../components/ui/PageHeader.vue";
import RepoRow from "../components/ui/RepoRow.vue";
import SurfaceCard from "../components/ui/SurfaceCard.vue";

const props = defineProps<{
  settings: Settings | null;
  busy: boolean;
}>();

const emit = defineEmits<{
  add: [path: string, enabled: boolean];
  toggle: [repositoryId: string, enabled: boolean];
  failed: [reason: unknown];
}>();

const { t } = useI18n();

/** 文件夹选择依赖 Tauri 插件，浏览器里直接打开 Vite 时隐藏该按钮。 */
const desktopAvailable = isDesktopAvailable();

const pathInput = ref("");
const picking = ref(false);
/**
 * 添加仓库时是否启用推送保护。
 *
 * 该值只对「新登记的仓库」生效：已登记的路径会沿用列表里的开关，
 * 重新添加只刷新 Hook 脚本（见 Rust 侧 `add_repository`）。
 */
const enableOnAdd = ref(false);

/** 设置没取到时列表是「未知」，不能展示成「没有仓库」。 */
const settingsReady = computed(() => props.settings !== null);
const repositories = computed(() => props.settings?.repositories ?? []);
const canAdd = computed(
  () => settingsReady.value && pathInput.value.trim().length > 0 && !props.busy,
);

/** 打开系统文件夹选择器；用户取消时保持输入框内容不变。 */
async function browse(): Promise<void> {
  if (picking.value || props.busy) {
    return;
  }

  picking.value = true;
  try {
    const directory = await pickRepositoryDirectory();
    if (directory !== null) {
      pathInput.value = directory;
    }
  } catch (reason) {
    emit("failed", reason);
  } finally {
    picking.value = false;
  }
}

function submitAdd(): void {
  if (!canAdd.value) {
    return;
  }
  // 是否启用保护由添加表单上的开关决定，不在后端硬编码。
  emit("add", pathInput.value.trim(), enableOnAdd.value);
}

/** 开关只发出请求，真实状态始终来自 Rust 返回的配置。 */
function requestToggle(repositoryId: string, enabled: boolean): void {
  emit("toggle", repositoryId, enabled);
}
</script>

<template>
  <section aria-labelledby="repositories-title">
    <PageHeader
      title-id="repositories-title"
      :title="t('repositories.title')"
      :subtitle="t('repositories.subtitle')"
    >
      <template #actions>
        <el-tag
          type="info"
          effect="plain"
          round
          disable-transitions
        >
          {{ t("common.count", { count: repositories.length }) }}
        </el-tag>
      </template>
    </PageHeader>

    <NoticeBanner
      v-if="!settingsReady"
      type="warning"
      :description="t('common.loadFailed')"
      class="mb-4"
    />

    <CardGrid>
      <SurfaceCard :title="t('repositories.listTitle')">
        <ul
          v-if="repositories.length > 0"
          class="m-0 list-none p-0"
        >
          <RepoRow
            v-for="repository in repositories"
            :key="repository.repositoryId"
            :repository="repository"
            :busy="busy"
            interactive
            @toggle="requestToggle"
          />
        </ul>
        <EmptyPanel
          v-else-if="!settingsReady"
          :title="t('common.loadFailed')"
        />
        <EmptyPanel
          v-else
          :title="t('repositories.empty')"
        >
          <template #icon>
            <FolderOpened />
          </template>
        </EmptyPanel>
      </SurfaceCard>

      <SurfaceCard :title="t('repositories.addTitle')">
        <form
          class="grid gap-3"
          data-add-form
          @submit.prevent="submitAdd"
        >
          <label
            class="grid gap-1.5"
            for="repository-path"
          >
            <span class="text-meta font-medium text-ink">
              {{ t("repositories.addLabel") }}
            </span>
            <el-input
              id="repository-path"
              v-model="pathInput"
              :placeholder="t('repositories.addPlaceholder')"
              :disabled="busy"
              clearable
              spellcheck="false"
              autocomplete="off"
            >
              <template #prefix>
                <el-icon>
                  <FolderOpened />
                </el-icon>
              </template>
            </el-input>
          </label>

          <div class="flex flex-wrap items-center gap-2">
            <el-button
              v-if="desktopAvailable"
              data-browse
              :loading="picking"
              :disabled="busy"
              @click="browse"
            >
              {{ picking ? t("repositories.browsing") : t("repositories.browseButton") }}
            </el-button>
            <el-button
              type="primary"
              native-type="submit"
              :disabled="!canAdd"
              :loading="busy"
            >
              {{ busy ? t("repositories.adding") : t("repositories.addButton") }}
            </el-button>
          </div>

          <div class="flex items-center gap-2">
            <el-switch
              v-model="enableOnAdd"
              :disabled="busy"
              :aria-label="t('repositories.addWithProtection')"
              data-add-protection
            />
            <span class="text-meta text-ink">
              {{ t("repositories.addWithProtection") }}
            </span>
          </div>

          <p class="m-0 rounded-lg bg-canvas px-3 py-2 text-caption leading-relaxed text-ink-muted">
            {{ t("repositories.note") }}
          </p>
        </form>
      </SurfaceCard>
    </CardGrid>
  </section>
</template>
