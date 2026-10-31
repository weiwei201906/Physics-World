<script setup lang="ts">
import { useRouter } from 'vue-router'
import { useSessionStore } from '@/stores/session'

const props = defineProps<{ chapterId: string }>()
const session = useSessionStore()
const router = useRouter()

async function startChapterSession(): Promise<void> {
  const id = await session.startSession('CHAPTER', props.chapterId)
  await router.push({ name: 'quiz-session', params: { sessionId: id } })
}
</script>

<template>
  <div class="chapter">
    <header class="page-head">
      <p class="eyebrow">Chapter drill</p>
      <h1>Chapter {{ chapterId }}</h1>
      <p class="desc">Focus your practice on a single chapter.</p>
    </header>

    <button class="btn btn-primary btn-lg" :disabled="session.loading" @click="startChapterSession">
      {{ session.loading ? 'Starting...' : 'Start chapter session' }}
    </button>
  </div>
</template>

<style scoped>
.chapter {
  max-width: 640px;
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
</style>
