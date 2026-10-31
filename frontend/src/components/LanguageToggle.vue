<script setup lang="ts">
import { locale, setLocale, availableLocales, t, type Locale } from '@/i18n'

const labels: Record<Locale, string> = {
  'zh-CN': t('common.langZh'),
  'en-US': t('common.langEn')
}

function cycle(): void {
  const index = availableLocales.indexOf(locale.value)
  const next = availableLocales[(index + 1) % availableLocales.length]
  setLocale(next)
}
</script>

<template>
  <button
    type="button"
    class="lang-toggle"
    :aria-label="t('common.language')"
    :title="t('common.language')"
    @click="cycle"
  >
    <span class="lang-toggle__label">{{ labels[locale] }}</span>
    <span class="lang-toggle__sep">/</span>
    <span class="lang-toggle__next">
      {{ labels[availableLocales[(availableLocales.indexOf(locale) + 1) % availableLocales.length]] }}
    </span>
  </button>
</template>

<style scoped>
.lang-toggle {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-rule-strong);
  border-radius: var(--radius-xs);
  background-color: transparent;
  color: var(--color-text-muted);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  white-space: nowrap;
  cursor: pointer;
}

.lang-toggle:hover {
  border-color: var(--color-accent);
  color: var(--color-accent-deep);
}

.lang-toggle__label {
  font-weight: 600;
  color: var(--color-text);
}

.lang-toggle__sep {
  color: var(--color-text-faint);
}

.lang-toggle__next {
  color: var(--color-text-muted);
}
</style>
