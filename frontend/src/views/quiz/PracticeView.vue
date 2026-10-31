<script setup lang="ts">
import { ref } from 'vue'
import { RouterLink, useRoute, useRouter } from 'vue-router'
import { t } from '@/i18n'
import { useSessionStore } from '@/stores/session'
import type { QuizMode } from '@/types'

const route = useRoute()
const router = useRouter()
const session = useSessionStore()

const initialMode: QuizMode = route.query.mode === 'EXAM' ? 'EXAM' : 'PRACTICE'
const mode = ref<QuizMode>(initialMode)
const error = ref('')

async function start(): Promise<void> {
  error.value = ''
  try {
    const id = await session.startSession(mode.value)
    await router.push({ name: 'quiz-session', params: { sessionId: id } })
  } catch {
    error.value = t('practice.error')
  }
}
</script>

<template>
  <div class="practice">
    <header class="page-head">
      <div>
        <p class="eyebrow">{{ t('practice.modeLabel') }}</p>
        <h1 class="page-title">{{ t('practice.title') }}</h1>
        <p class="page-desc">{{ t('practice.desc') }}</p>
      </div>
    </header>

    <div class="grid-12 modes">
      <button
        type="button"
        class="mode-card span-7"
        :class="{ 'mode-card--active': mode === 'PRACTICE' }"
        :aria-pressed="mode === 'PRACTICE'"
        @click="mode = 'PRACTICE'"
      >
        <span class="mode-index mono">01</span>
        <span class="mode-title">{{ t('practice.modePractice') }}</span>
        <span class="mode-desc">{{ t('practice.modePracticeDesc') }}</span>
      </button>

      <button
        type="button"
        class="mode-card span-5"
        :class="{ 'mode-card--active': mode === 'EXAM' }"
        :aria-pressed="mode === 'EXAM'"
        @click="mode = 'EXAM'"
      >
        <span class="mode-index mono">02</span>
        <span class="mode-title">{{ t('practice.modeExam') }}</span>
        <span class="mode-desc">{{ t('practice.modeExamDesc') }}</span>
      </button>
    </div>

    <div class="notes">
      <p class="notice">{{ t('practice.timerNote') }}</p>
      <p class="chapter-hint">
        {{ t('practice.chapterHint') }}
        <RouterLink to="/chapters">{{ t('practice.goChapters') }}</RouterLink>
      </p>
    </div>

    <p v-if="error" class="form-error" role="alert">{{ error }}</p>

    <footer class="actions">
      <button class="btn btn-accent btn-lg" :disabled="session.loading" @click="start">
        {{ session.loading ? t('practice.starting') : t('practice.begin') }}
      </button>
    </footer>
  </div>
</template>

<style scoped>
.practice {
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
}

.modes {
  align-items: stretch;
}

.mode-card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-2);
  padding: var(--space-5);
  border: 1px solid var(--color-rule-strong);
  border-radius: var(--radius-xs);
  background-color: var(--color-surface);
  text-align: left;
  color: var(--color-text);
}

.mode-card:hover {
  border-color: var(--color-text);
}

.mode-card--active {
  border-color: var(--color-accent);
  background-color: var(--color-surface);
}

.mode-card--active .mode-title {
  color: var(--color-accent-deep);
}

.mode-index {
  font-size: var(--text-xs);
  letter-spacing: 0.12em;
  color: var(--color-text-muted);
}

.mode-title {
  font-family: var(--font-display);
  font-size: var(--text-xl);
  font-weight: 600;
}

.mode-desc {
  max-width: 36ch;
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}

.notes {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.chapter-hint {
  margin: 0;
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}

.actions {
  display: flex;
  justify-content: flex-end;
}

@media (max-width: 720px) {
  .mode-card {
    padding: var(--space-4);
  }

  .actions {
    justify-content: stretch;
  }

  .actions .btn {
    width: 100%;
  }
}
</style>
