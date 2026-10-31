<script setup lang="ts">
import { t } from '@/i18n'

withDefaults(defineProps<{ message?: string }>(), { message: '' })

defineEmits<{ retry: [] }>()
</script>

<template>
  <div class="error-state" role="alert">
    <span class="error-mark" aria-hidden="true">!</span>
    <div class="error-body">
      <p class="error-title">{{ t('common.error') }}</p>
      <p class="error-message">{{ message || t('common.unknownError') }}</p>
    </div>
    <button type="button" class="btn btn-secondary btn-sm" @click="$emit('retry')">
      {{ t('common.retry') }}
    </button>
  </div>
</template>

<style scoped>
.error-state {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  padding: var(--space-4);
  border: 1px solid var(--color-wrong);
  border-radius: var(--radius-sm);
  background-color: var(--color-wrong-bg);
}

.error-mark {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 1.5rem;
  height: 1.5rem;
  border: 1px solid var(--color-wrong);
  border-radius: 50%;
  color: var(--color-wrong);
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  flex-shrink: 0;
}

.error-body {
  flex: 1;
  min-width: 0;
}

.error-title {
  margin: 0;
  font-weight: 600;
  color: var(--color-wrong);
  font-size: var(--text-sm);
}

.error-message {
  margin: 0;
  color: var(--color-text-muted);
  font-size: var(--text-sm);
  overflow-wrap: anywhere;
}

@media (max-width: 600px) {
  .error-state {
    flex-wrap: wrap;
    align-items: flex-start;
  }
}
</style>
