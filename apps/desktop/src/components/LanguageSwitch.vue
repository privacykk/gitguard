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
  <div class="language-switch">
    <label class="language-switch__label" for="locale-select">
      {{ t("language.label") }}
    </label>
    <select id="locale-select" v-model="selectedLocale" class="language-switch__select">
      <option v-for="option in SUPPORTED_LOCALES" :key="option" :value="option">
        {{ LOCALE_LABELS[option] }}
      </option>
    </select>
  </div>
</template>

<style scoped>
.language-switch {
  display: flex;
  gap: 0.5rem;
  align-items: center;
  justify-content: flex-end;
  font-size: 0.875rem;
}

.language-switch__select {
  padding: 0.25rem 0.5rem;
  border: 1px solid #d7dee8;
  border-radius: 0.375rem;
  background: #ffffff;
}
</style>
