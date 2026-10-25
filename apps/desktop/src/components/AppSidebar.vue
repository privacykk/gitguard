<script setup lang="ts">
/**
 * 侧边栏：品牌区 + 折叠按钮 + 业务导航 + 底部设置入口。
 *
 * 导航用 Element Plus 的 `el-menu`（`:collapse` 控制收起），收起状态由
 * `App.vue` 持有，这里只负责发出意图——布局宽度跟着状态走，不会不同步。
 * 收起后标签用 `aria-label` / `title` 保留可读名称，图标仍能独立辨识页面。
 */
import {
  Document,
  Expand,
  Fold,
  FolderOpened,
  Odometer,
  Operation,
  Setting,
  Ticket,
  User,
} from "@element-plus/icons-vue";
import { type Component, computed } from "vue";
import { useI18n } from "vue-i18n";
import { NAV_LABEL_KEYS, NAV_VIEWS, type AppView, type NavView } from "../navigation";
import { settingsShortcutLabel } from "../platform";

const props = defineProps<{
  current: AppView;
  /** 是否有待确认的推送请求；只影响提示点，不参与任何判断。 */
  pending: boolean;
  /** 收起后只保留图标，为内容区腾出宽度。 */
  collapsed: boolean;
}>();

const emit = defineEmits<{
  navigate: [view: AppView];
  toggleCollapse: [];
}>();

const { t } = useI18n();

const shortcut = settingsShortcutLabel();

const NAV_ICONS: Record<NavView, Component> = {
  dashboard: Odometer,
  identities: User,
  repositories: FolderOpened,
  policies: Operation,
  approval: Ticket,
  audit: Document,
};

const toggleLabel = computed(() =>
  props.collapsed ? t("nav.expandSidebar") : t("nav.collapseSidebar"),
);

function isPending(view: NavView): boolean {
  return view === "approval" && props.pending;
}

/** el-menu 的 select 回传的是 index 字符串，这里收窄回页面联合类型。 */
function handleSelect(index: string): void {
  emit("navigate", index as AppView);
}
</script>

<template>
  <aside
    :class="[
      'flex h-full shrink-0 flex-col gap-4 border-r border-line bg-surface transition-[width] duration-200 ease-out',
      collapsed ? 'w-19 px-2 py-4' : 'w-60 px-3 py-4',
    ]"
  >
    <div
      :class="[
        'flex gap-2',
        collapsed ? 'flex-col items-center' : 'items-center justify-between px-1',
      ]"
    >
      <div class="flex min-w-0 items-center gap-2.5">
        <span
          class="grid size-8 shrink-0 place-items-center rounded-[0.625rem] bg-brand-600 text-sm font-bold text-white"
          aria-hidden="true"
        >
          G
        </span>
        <div
          v-if="!collapsed"
          class="min-w-0"
        >
          <p class="m-0 text-sm font-bold text-ink">
            {{ t("app.title") }}
          </p>
          <p class="m-0 mt-0.5 text-micro text-ink-subtle">
            {{ t("app.subtitle") }}
          </p>
        </div>
      </div>

      <!-- 不用 el-tooltip：它会用自己的展开状态覆盖按钮上的 aria-expanded。 -->
      <el-button
        text
        circle
        size="small"
        data-sidebar-toggle
        :title="toggleLabel"
        :aria-expanded="!collapsed"
        :aria-label="toggleLabel"
        @click="emit('toggleCollapse')"
      >
        <el-icon>
          <Fold v-if="!collapsed" />
          <Expand v-else />
        </el-icon>
      </el-button>
    </div>

    <el-menu
      :default-active="current"
      :collapse="collapsed"
      :collapse-transition="false"
      class="sidebar-menu flex-1 border-r-0 bg-transparent"
      @select="handleSelect"
    >
      <el-menu-item
        v-for="view in NAV_VIEWS"
        :key="view"
        :index="view"
        :data-view="view"
        :aria-current="current === view ? 'page' : undefined"
        :aria-label="collapsed ? t(NAV_LABEL_KEYS[view]) : undefined"
        :title="collapsed ? t(NAV_LABEL_KEYS[view]) : undefined"
      >
        <el-badge
          :is-dot="isPending(view) && collapsed"
          type="warning"
          class="menu-icon"
        >
          <el-icon>
            <component :is="NAV_ICONS[view]" />
          </el-icon>
        </el-badge>
        <span
          v-if="!collapsed"
          class="flex min-w-0 flex-1 items-center gap-2"
        >
          <span class="truncate">{{ t(NAV_LABEL_KEYS[view]) }}</span>
          <span
            v-if="isPending(view)"
            class="rounded-full bg-warning-50 px-1.5 py-px text-micro font-semibold text-warning-600"
          >
            {{ t("nav.pending") }}
          </span>
        </span>
      </el-menu-item>
    </el-menu>

    <div
      :class="[
        'border-t border-line pt-3',
        collapsed ? 'flex flex-col items-center gap-2' : 'grid gap-2',
      ]"
    >
      <el-menu
        :default-active="current === 'settings' ? 'settings' : ''"
        :collapse="collapsed"
        :collapse-transition="false"
        class="sidebar-menu border-r-0 bg-transparent"
        @select="handleSelect"
      >
        <el-menu-item
          index="settings"
          data-view="settings"
          :aria-current="current === 'settings' ? 'page' : undefined"
          :aria-label="t('nav.settingsShortcut', { shortcut })"
          :title="collapsed ? t('nav.settings') : undefined"
        >
          <el-icon>
            <Setting />
          </el-icon>
          <span
            v-if="!collapsed"
            class="flex min-w-0 flex-1 items-center justify-between gap-2"
          >
            <span>{{ t("nav.settings") }}</span>
            <kbd
              class="rounded border border-line bg-canvas px-1.5 py-px font-sans text-micro text-ink-subtle"
            >{{ shortcut }}</kbd>
          </span>
        </el-menu-item>
      </el-menu>
      <p
        v-if="!collapsed"
        class="m-0 px-3 text-micro text-ink-subtle"
      >
        {{ t("app.productStage") }}
      </p>
    </div>
  </aside>
</template>

<style scoped>
/* Element Plus 菜单默认带右边框与白色背景，这里改成贴合侧边栏的样式。 */
.sidebar-menu {
  border-right: 0;
  background: transparent;
}

.sidebar-menu :deep(.el-menu-item) {
  height: 2.375rem;
  line-height: 2.375rem;
  margin-bottom: 0.125rem;
  border-radius: 0.5rem;
  color: var(--color-ink);
  font-size: 0.875rem;
}

.sidebar-menu :deep(.el-menu-item:hover) {
  background: rgb(59 92 204 / 0.07);
}

.sidebar-menu :deep(.el-menu-item.is-active) {
  background: var(--color-brand-50);
  color: var(--color-brand-700);
  font-weight: 600;
}

/* 收起态：整行居中、去掉内边距，图标才有足够的点按空间。 */
.sidebar-menu :deep(.el-menu--collapse) {
  width: 100%;
}

.sidebar-menu :deep(.el-menu--collapse .el-menu-item) {
  justify-content: center;
  padding: 0 !important;
}

.menu-icon :deep(.el-badge__content.is-dot) {
  top: 0.25rem;
  right: 0.5rem;
}
</style>
