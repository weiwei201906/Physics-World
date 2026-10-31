<script setup lang="ts">
import { ref } from 'vue'

const activeTab = ref<'questions' | 'config'>('questions')
</script>

<template>
  <div class="admin">
    <header class="page-head">
      <p class="eyebrow">Admin</p>
      <h1>Console</h1>
    </header>

    <div class="tabs" role="tablist">
      <button
        class="tab"
        :class="{ 'tab--active': activeTab === 'questions' }"
        @click="activeTab = 'questions'"
      >
        Question bank
      </button>
      <button
        class="tab"
        :class="{ 'tab--active': activeTab === 'config' }"
        @click="activeTab = 'config'"
      >
        Configuration
      </button>
    </div>

    <div v-if="activeTab === 'questions'" class="panel">
      <p>Upload and manage physics questions. Updates propagate to the cache within seconds.</p>
      <button class="btn btn-primary">Upload question</button>
    </div>

    <div v-else class="panel">
      <p>Edit runtime configuration. Changes are validated against the registered schema and applied without restart.</p>
      <button class="btn btn-secondary">Open config editor</button>
    </div>
  </div>
</template>

<style scoped>
.admin {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
}

.eyebrow {
  font-size: var(--text-sm);
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--color-primary-700);
  font-weight: 600;
  margin: 0 0 var(--space-2);
}

.tabs {
  display: flex;
  gap: var(--space-1);
  border-bottom: 1px solid var(--color-border);
}

.tab {
  padding: var(--space-2) var(--space-4);
  font-weight: 500;
  color: var(--color-text-muted);
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
  transition: color var(--duration-fast) var(--ease-out),
              border-color var(--duration-fast) var(--ease-out);
}

.tab--active {
  color: var(--color-primary-700);
  border-bottom-color: var(--color-primary-600);
}

.panel {
  padding: var(--space-5);
  background-color: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  align-items: flex-start;
}

.panel p {
  color: var(--color-text-muted);
  margin: 0;
}
</style>
