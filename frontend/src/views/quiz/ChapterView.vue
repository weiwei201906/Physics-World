<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink, useRouter } from 'vue-router'
import { t } from '@/i18n'
import { fetchChapters } from '@/api/user'
import { useSessionStore } from '@/stores/session'
import type { Chapter } from '@/types'
import LoadingSkeleton from '@/components/LoadingSkeleton.vue'
import EmptyState from '@/components/EmptyState.vue'
import ErrorState from '@/components/ErrorState.vue'

const props = defineProps<{ chapterId?: string }>()
const router = useRouter()
const session = useSessionStore()

const chapters = ref<Chapter[]>([])
const loading = ref(true)
const failed = ref(false)
const startError = ref('')

const isDetail = computed(() => !!props.chapterId)
const current = computed(() =>
  chapters.value.find((chapter) => chapter.id === props.chapterId) ?? null
)

async function load(): Promise<void> {
  loading.value = true
  failed.value = false
  try {
    chapters.value = await fetchChapters()
  } catch {
    failed.value = true
  } finally {
    loading.value = false
  }
}

async function startChapter(): Promise<void> {
  if (!props.chapterId) return
  startError.value = ''
  try {
    const id = await session.startSession('CHAPTER', props.chapterId)
    await router.push({ name: 'quiz-session', params: { sessionId: id } })
  } catch {
    startError.value = t('chapter.sessionError')
  }
}

onMounted(load)
</script>

<template>
  <div v-if="isDetail" class="chapter-detail stack">
    <header class="page-head">
      <div>
        <p class="eyebrow">{{ t('chapter.detailTitle') }}</p>
        <h1 class="page-title">
          {{ current?.name ?? t('chapter.unknownName') }}
        </h1>
        <p class="page-desc">
          {{ t('chapter.detailDesc') }}
        </p>
      </div>
      <RouterLink to="/chapters" class="btn btn-secondary">{{ t('chapter.backToList') }}</RouterLink>
    </header>

    <LoadingSkeleton v-if="loading" :lines="3" />
    <ErrorState v-else-if="failed" :message="t('chapter.error')" @retry="load" />
    <EmptyState v-else-if="!current" :title="t('chapter.notFound')" />

    <section v-else class="chapter-card">
      <div class="chapter-facts">
        <span class="label">{{ t('chapter.name') }}</span>
        <span class="fact-value">{{ current.name }}</span>
        <span class="label">{{ t('chapter.questionCount') }}</span>
        <span class="fact-value mono">{{ t('chapter.count', { count: current.questionCount }) }}</span>
      </div>
      <p class="chapter-note">{{ t('practice.timerNote') }}</p>
      <p v-if="startError" class="form-error" role="alert">{{ startError }}</p>
      <button class="btn btn-accent btn-lg" :disabled="session.loading" @click="startChapter">
        {{ session.loading ? t('chapter.starting') : t('chapter.start') }}
      </button>
    </section>
  </div>

  <div v-else class="chapter-list">
    <header class="page-head">
      <div>
        <p class="eyebrow">{{ t('nav.chapters') }}</p>
        <h1 class="page-title">{{ t('chapter.listTitle') }}</h1>
        <p class="page-desc">{{ t('chapter.listDesc') }}</p>
      </div>
    </header>

    <LoadingSkeleton v-if="loading" :lines="5" />
    <ErrorState v-else-if="failed" :message="t('chapter.error')" @retry="load" />
    <EmptyState v-else-if="!chapters.length" :title="t('chapter.empty')" />
    <div v-else class="ledger chapter-ledger">
      <div class="ledger-row ledger-head chapter-row">
        <span>{{ t('chapter.index') }}</span>
        <span>{{ t('chapter.name') }}</span>
        <span>{{ t('chapter.questionCount') }}</span>
        <span>{{ t('chapter.openAction') }}</span>
      </div>
      <RouterLink
        v-for="(chapter, index) in chapters"
        :key="chapter.id"
        :to="{ name: 'chapter', params: { chapterId: chapter.id } }"
        class="ledger-row chapter-row chapter-link"
      >
        <span class="mono">{{ (index + 1).toString().padStart(2, '0') }}</span>
        <span class="chapter-name">{{ chapter.name }}</span>
        <span class="mono">{{ t('chapter.count', { count: chapter.questionCount }) }}</span>
        <span class="text-link">{{ t('chapter.open') }}</span>
      </RouterLink>
    </div>
  </div>
</template>

<style scoped>
.stack {
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
}

.chapter-card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-4);
  max-width: 40rem;
  padding: var(--space-6);
  border: 1px solid var(--color-rule);
  border-radius: var(--radius-sm);
  background-color: var(--color-surface);
}

.chapter-facts {
  display: grid;
  grid-template-columns: 8rem minmax(0, 1fr);
  gap: var(--space-2) var(--space-4);
  width: 100%;
  padding-bottom: var(--space-4);
  border-bottom: 1px solid var(--color-rule);
}

.fact-value {
  font-family: var(--font-display);
  font-size: var(--text-lg);
}

.chapter-note {
  margin: 0;
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}

.chapter-row {
  grid-template-columns: 3rem minmax(0, 1fr) 7rem 5rem;
}

.chapter-link {
  color: var(--color-text);
  text-decoration: none;
}

.chapter-link:hover {
  background-color: var(--color-bg-deep);
  text-decoration: none;
}

.chapter-name {
  font-family: var(--font-display);
  font-size: var(--text-base);
}

@media (max-width: 720px) {
  .chapter-row {
    grid-template-columns: 2.5rem minmax(0, 1fr) auto;
  }

  .chapter-row > :nth-child(3) {
    display: none;
  }

  .chapter-facts {
    grid-template-columns: 1fr;
  }
}
</style>
