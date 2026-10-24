<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import DevelopmentStatus from './components/DevelopmentStatus.vue'
import LanguageSwitch from './components/LanguageSwitch.vue'

const { t } = useI18n()
</script>

<template>
  <main class="shell">
    <LanguageSwitch />
    <p class="eyebrow">
      {{ t('app.eyebrow') }}
    </p>
    <h1>{{ t('app.title') }}</h1>
    <p class="intro">
      {{ t('app.intro') }}
    </p>
    <DevelopmentStatus />
  </main>
</template>
