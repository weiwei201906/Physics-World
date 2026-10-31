<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useConfigStore } from '@/stores/config'

const router = useRouter()
const config = useConfigStore()
const mode = ref<string>('1v1')
const matching = ref(false)

function findMatch(): void {
  matching.value = true
  // In a real implementation this calls battle-service matching.
  // For now, navigate to a demo room.
  setTimeout(() => {
    void router.push({ name: 'battle-room', params: { roomId: 'demo-room' } })
  }, 1500)
}
</script>

<template>
  <div class="battle-lobby">
    <header class="page-head">
      <p class="eyebrow">Battle</p>
      <h1>Challenge another student</h1>
      <p class="desc">Race through questions. Faster correct answers score higher.</p>
    </header>

    <div v-if="!config.config.features.battleEnabled" class="disabled">
      Battle mode is temporarily disabled.
    </div>

    <div v-else class="lobby-card">
      <div class="mode-row">
        <label class="mode-label">Match mode</label>
        <div class="mode-options">
          <button
            class="mode-btn"
            :class="{ 'mode-btn--active': mode === '1v1' }"
            @click="mode = '1v1'"
          >
            1 vs 1
          </button>
          <button
            class="mode-btn"
            :class="{ 'mode-btn--active': mode === 'squad' }"
            @click="mode = 'squad'"
          >
            Squad
          </button>
        </div>
      </div>

      <button class="btn btn-primary btn-lg btn-block" :disabled="matching" @click="findMatch">
        {{ matching ? 'Finding opponent...' : 'Find a match' }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.battle-lobby {
  max-width: 560px;
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

.desc {
  color: var(--color-text-muted);
  margin: var(--space-2) 0 0;
}

.disabled {
  padding: var(--space-5);
  background-color: var(--color-warning-bg);
  color: var(--color-warning);
  border-radius: var(--radius-md);
}

.lobby-card {
  padding: var(--space-5);
  background-color: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
}

.mode-row {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.mode-label {
  font-size: var(--text-sm);
  font-weight: 500;
  color: var(--color-text);
}

.mode-options {
  display: flex;
  gap: var(--space-2);
}

.mode-btn {
  flex: 1;
  padding: var(--space-3);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  background-color: var(--color-surface);
  font-weight: 500;
  color: var(--color-text-muted);
  transition: all var(--duration-fast) var(--ease-out);
}

.mode-btn--active {
  border-color: var(--color-primary-600);
  color: var(--color-primary-700);
  background-color: var(--color-primary-50);
}
</style>
