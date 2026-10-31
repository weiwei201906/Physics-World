<script setup lang="ts">
import { onMounted, computed } from 'vue'
import { useRouter } from 'vue-router'
import { useSessionStore } from '@/stores/session'

const props = defineProps<{ sessionId: string }>()
const session = useSessionStore()
const router = useRouter()

onMounted(() => {
  void session.loadSession(props.sessionId)
})

const selected = computed(() => session.selectedAnswer)

function selectOption(optionId: string): void {
  if (session.submitting) return
  session.selectedAnswer = optionId
}

async function submit(): Promise<void> {
  if (!session.selectedAnswer) return
  await session.submitAnswer(session.selectedAnswer)
}

async function next(): Promise<void> {
  if (session.isLastQuestion) {
    session.reset()
    await router.push({ name: 'home' })
    return
  }
  await session.nextQuestion()
}
</script>

<template>
  <div class="quiz">
    <header class="quiz-head">
      <div class="progress">
        <div class="progress-track">
          <div class="progress-fill" :style="{ width: `${session.progress}%` }" />
        </div>
        <span class="progress-text">
          {{ (session.currentSession?.currentIndex ?? 0) + 1 }} /
          {{ session.currentSession?.totalQuestions ?? 0 }}
        </span>
      </div>
    </header>

    <div v-if="session.loading" class="loading">Loading question...</div>

    <article v-else-if="session.currentQuestion" class="question">
      <p class="question-stem">{{ session.currentQuestion.content.stem }}</p>

      <img
        v-if="session.currentQuestion.content.imageUrl"
        :src="session.currentQuestion.content.imageUrl"
        alt=""
        class="question-image"
      />

      <div class="options">
        <button
          v-for="option in session.currentQuestion.content.options"
          :key="option.id"
          class="option"
          :class="{ 'option--selected': selected === option.id }"
          :disabled="session.submitting"
          @click="selectOption(option.id)"
        >
          <span class="option-label">{{ option.label }}</span>
          <span class="option-text">{{ option.text }}</span>
        </button>
      </div>

      <div v-if="session.selectedAnswer" class="explanation">
        <p>{{ session.currentQuestion.content.explanation }}</p>
      </div>

      <footer class="quiz-actions">
        <button
          v-if="!session.selectedAnswer"
          class="btn btn-primary"
          :disabled="!selected || session.submitting"
          @click="submit"
        >
          Submit answer
        </button>
        <button
          v-else
          class="btn btn-primary"
          @click="next"
        >
          {{ session.isLastQuestion ? 'Finish' : 'Next question' }}
        </button>
      </footer>
    </article>
  </div>
</template>

<style scoped>
.quiz {
  max-width: 760px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
}

.quiz-head {
  display: flex;
  align-items: center;
  gap: var(--space-4);
}

.progress {
  flex: 1;
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.progress-track {
  flex: 1;
  height: 6px;
  background-color: var(--color-bg-muted);
  border-radius: 3px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background-color: var(--color-primary-600);
  border-radius: 3px;
  transition: width var(--duration-base) var(--ease-out);
}

.progress-text {
  font-size: var(--text-sm);
  color: var(--color-text-muted);
  font-variant-numeric: tabular-nums;
}

.loading {
  padding: var(--space-7);
  text-align: center;
  color: var(--color-text-muted);
}

.question {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
}

.question-stem {
  font-family: var(--font-display);
  font-size: var(--text-xl);
  font-weight: 600;
  line-height: 1.4;
  color: var(--color-text);
  margin: 0;
}

.question-image {
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border);
  max-height: 320px;
  object-fit: contain;
}

.options {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.option {
  display: flex;
  align-items: flex-start;
  gap: var(--space-3);
  padding: var(--space-4);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background-color: var(--color-surface);
  text-align: left;
  transition: border-color var(--duration-fast) var(--ease-out),
              background-color var(--duration-fast) var(--ease-out);
}

.option:hover:not(:disabled) {
  border-color: var(--color-border-strong);
}

.option--selected {
  border-color: var(--color-primary-600);
  background-color: var(--color-primary-50);
}

.option-label {
  font-weight: 600;
  color: var(--color-primary-700);
  flex-shrink: 0;
}

.option-text {
  color: var(--color-text);
  line-height: 1.5;
}

.explanation {
  padding: var(--space-4);
  background-color: var(--color-bg-muted);
  border-radius: var(--radius-md);
  border-left: 3px solid var(--color-info);
}

.explanation p {
  margin: 0;
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}

.quiz-actions {
  display: flex;
  justify-content: flex-end;
}
</style>
