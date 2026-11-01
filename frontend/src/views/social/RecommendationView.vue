<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { t } from '@/i18n'
import { useSessionStore } from '@/stores/session'
import { fetchRecommendedQuestions } from '@/api/recommendation'
import { fetchChapters } from '@/api/user'
import type { Question } from '@/types'

const session = useSessionStore()
const router = useRouter()
const questions = ref<Question[]>([])
const chapterNames = ref<Record<string, string>>({})
const loading = ref(true)
const failed = ref(false)

onMounted(async () => {
  try {
    const [recommended, chapters] = await Promise.all([
      fetchRecommendedQuestions(),
      fetchChapters().catch(() => [])
    ])
    questions.value = recommended
    chapterNames.value = Object.fromEntries(chapters.map((chapter) => [chapter.id, chapter.name]))
  } catch {
    failed.value = true
  } finally {
    loading.value = false
  }
})

function chapterName(id: string): string {
  return chapterNames.value[id] ?? t('recommendation.chapterLabel', { id })
}

async function startFromQuestion(question: Question): Promise<void> {
  const id = await session.startSession('PRACTICE', question.chapterId)
  await router.push({ name: 'quiz-session', params: { sessionId: id } })
}
</script>

<template>
  <div class="recommendation">
    <header class="page-head">
      <p class="eyebrow">{{ t('nav.recommendation') }}</p>
      <h1>{{ t('recommendation.title') }}</h1>
      <p class="desc">{{ t('recommendation.desc') }}</p>
    </header>

    <div v-if="loading" class="loading">{{ t('recommendation.loading') }}</div>

    <div v-else-if="failed" class="empty">{{ t('recommendation.error') }}</div>

    <div v-else-if="questions.length === 0" class="empty">
      {{ t('recommendation.empty') }}
    </div>

    <div v-else class="question-list">
      <h2 class="list-title">{{ t('recommendation.listTitle') }}</h2>
      <button
        v-for="(q, index) in questions"
        :key="q.id"
        class="question-item"
        @click="startFromQuestion(q)"
      >
        <span class="question-index">{{ index + 1 }}</span>
        <span class="question-stem">{{ q.content.stem }}</span>
        <span class="question-chapter">{{ chapterName(q.chapterId) }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.recommendation {
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

.loading,
.empty {
  padding: var(--space-7);
  text-align: center;
  color: var(--color-text-muted);
}

.question-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.list-title {
  margin: 0;
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--color-text-muted);
}

.question-item {
  display: grid;
  grid-template-columns: 32px 1fr auto;
  align-items: center;
  gap: var(--space-4);
  padding: var(--space-4);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background-color: var(--color-surface);
  text-align: left;
  transition: border-color var(--duration-fast) var(--ease-out);
}

.question-item:hover {
  border-color: var(--color-primary-600);
}

.question-index {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  background-color: var(--color-bg-muted);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--color-text-muted);
}

.question-stem {
  color: var(--color-text);
  font-size: var(--text-base);
  line-height: 1.4;
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}

.question-chapter {
  font-size: var(--text-xs);
  color: var(--color-text-faint);
  white-space: nowrap;
  max-width: 12rem;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
