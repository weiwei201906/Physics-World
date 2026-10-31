<script setup lang="ts">
import { t } from '@/i18n'
import { useToast } from '@/composables/useToast'

const { toasts, dismissToast } = useToast()
</script>

<template>
  <div class="toast-stack" role="region" aria-live="polite" aria-label="Notifications">
    <div
      v-for="toast in toasts"
      :key="toast.id"
      class="toast"
      :class="`toast--${toast.tone}`"
      :role="toast.tone === 'error' ? 'alert' : 'status'"
    >
      <span class="toast-mark" aria-hidden="true" />
      <p class="toast-message">{{ toast.message }}</p>
      <button class="toast-close" type="button" :aria-label="t('toast.dismiss')" @click="dismissToast(toast.id)">
        <span aria-hidden="true">x</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.toast-stack {
  position: fixed;
  z-index: 40;
  right: var(--space-5);
  bottom: var(--space-5);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  width: min(22rem, calc(100vw - 2rem));
  pointer-events: none;
}

.toast {
  display: grid;
  grid-template-columns: 10px 1fr auto;
  align-items: start;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border: 1px solid var(--color-rule-strong);
  border-radius: var(--radius-sm);
  background-color: var(--color-surface);
  color: var(--color-text);
  box-shadow: 0 6px 18px rgba(20, 17, 14, 0.14);
  pointer-events: auto;
}

.toast-mark {
  width: 6px;
  height: 6px;
  margin-top: 0.4rem;
  background-color: var(--color-text-muted);
}

.toast--success .toast-mark { background-color: var(--color-correct); }
.toast--warning .toast-mark { background-color: var(--color-warning); }
.toast--error .toast-mark { background-color: var(--color-wrong); }

.toast-message {
  margin: 0;
  font-size: var(--text-sm);
  line-height: 1.5;
}

.toast-close {
  width: 1.5rem;
  height: 1.5rem;
  border: 1px solid transparent;
  border-radius: var(--radius-xs);
  color: var(--color-text-muted);
  font-family: var(--font-mono);
  font-size: var(--text-sm);
}

.toast-close:hover {
  border-color: var(--color-rule-strong);
  color: var(--color-text);
}

@media (max-width: 720px) {
  .toast-stack {
    right: var(--space-4);
    left: var(--space-4);
    width: auto;
  }
}
</style>
