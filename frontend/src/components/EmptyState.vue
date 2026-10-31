<script setup lang="ts">
import { t } from '@/i18n'

withDefaults(defineProps<{ title?: string; message?: string }>(), {
  title: '',
  message: ''
})
</script>

<template>
  <div class="empty-state" role="status">
    <svg viewBox="0 0 48 32" width="48" height="32" fill="none" aria-hidden="true">
      <path d="M2 30 H46" stroke="currentColor" stroke-width="1.5" stroke-linecap="square" />
      <path d="M8 30 C14 30 15 8 24 8 C33 8 34 18 42 22" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
    </svg>
    <p class="empty-title">{{ title || t('common.empty') }}</p>
    <p v-if="message" class="empty-message">{{ message }}</p>
    <slot />
  </div>
</template>

<style scoped>
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-2);
  padding: var(--space-6);
  border: 1px dashed var(--color-rule-strong);
  border-radius: var(--radius-sm);
  color: var(--color-text-muted);
}

.empty-state svg {
  color: var(--color-rule-strong);
  margin-bottom: var(--space-2);
}

.empty-title {
  margin: 0;
  color: var(--color-text);
  font-family: var(--font-display);
  font-size: var(--text-lg);
}

.empty-message {
  margin: 0;
  font-size: var(--text-sm);
}
</style>
