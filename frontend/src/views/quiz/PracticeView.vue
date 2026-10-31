<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useSessionStore } from '@/stores/session'

const session = useSessionStore()
const router = useRouter()

const mode = ref<'PRACTICE' | 'EXAM'>('PRACTICE')

async function start(): Promise<void> {
  const id = await session.startSession(mode.value)
  await router.push({ name: 'quiz-session', params: { sessionId: id } })
}
</script>

<template>
  <div class="practice">
    <header class="page-head">
      <p class="eyebrow">Practice</p>
      <h1>Pick your mode</h1>
      <p class="desc">Choose how you want to work through questions right now.</p>
    </header>

    <div class="mode-grid">
      <button
        class="mode-card"
        :class="{ 'mode-card--active': mode === 'PRACTICE' }"
        @click="mode = 'PRACTICE'"
      >
        <span class="mode-title">Practice</span>
        <span class="mode-desc">Untimed. Reveal explanations as you go. Best for learning.</span>
      </button>
      <button
        class="mode-card"
        :class="{ 'mode-card--active': mode === 'EXAM' }"
        @click="mode = 'EXAM'"
      >
        <span class="mode-title">Exam</span>
        <span class="mode-desc">Timed session. Score counts toward your leaderboard rank.</span>
      </button>
    </div>

    <div class="actions">
      <button class="btn btn-primary btn-lg" :disabled="session.loading" @click="start">
        {{ session.loading ? 'Starting...' : 'Begin session' }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.practice {
  max-width: 720px;
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
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

.mode-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-4);
}

.mode-card {
  text-align: left;
  padding: var(--space-5);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  background-color: var(--color-surface);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  transition: border-color var(--duration-fast) var(--ease-out);
}

.mode-card:hover {
  border-color: var(--color-border-strong);
}

.mode-card--active {
  border-color: var(--color-primary-600);
  box-shadow: var(--shadow-sm);
}

.mode-title {
  font-family: var(--font-display);
  font-weight: 600;
  font-size: var(--text-lg);
  color: var(--color-text);
}

.mode-desc {
  font-size: var(--text-sm);
  color: var(--color-text-muted);
  line-height: 1.5;
}

.actions {
  display: flex;
  justify-content: flex-end;
}

@media (max-width: 600px) {
  .mode-grid {
    grid-template-columns: 1fr;
  }
}
</style>
