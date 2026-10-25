<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import {
  SUPPORTED_LOCALES,
  isSupportedLocale,
  setLocale,
  type SupportedLocale,
} from "../i18n";

// 语言名称使用各语言的自称，不随界面语言翻译。
const LOCALE_LABELS: Record<SupportedLocale, string> = {
  "zh-CN": "简体中文",
  "en-US": "English",
};

const { t, locale } = useI18n();

// 用 v-model 双向绑定：读取当前语言，选择后交给 setLocale（含持久化）。
const selectedLocale = computed({
  get: () => locale.value,
  set: (value: string) => {
    if (isSupportedLocale(value)) {
      setLocale(value);
    }
  },
});
</script>

<template>
  <div class="flex flex-wrap items-center gap-2">
    <!-- 外层已有「界面语言」标题，这里保留隐藏 label 供辅助技术识别下拉框。 -->
    <label
      class="sr-only"
      for="locale-select"
    >
      {{ t("language.label") }}
    </label>
    <select
      id="locale-select"
      v-model="selectedLocale"
      class="h-8 max-w-56 rounded-md border border-line bg-surface px-2.5 text-sm text-ink transition-colors hover:border-brand-500 focus:border-brand-600"
    >
      <option
        v-for="option in SUPPORTED_LOCALES"
        :key="option"
        :value="option"
      >
        {{ LOCALE_LABELS[option] }}
      </option>
    </select>
  </div>
</template>
