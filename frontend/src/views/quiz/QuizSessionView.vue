<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { RouterLink, useRouter } from 'vue-router'
import { t } from '@/i18n'
import { useSessionStore } from '@/stores/session'
import { useProfileStore } from '@/stores/profile'
import { useConfigStore } from '@/stores/config'
import { useToast } from '@/composables/useToast'
import { computeRemainingSeconds } from '@/composables/useExamTimer'
import LoadingSkeleton from '@/components/LoadingSkeleton.vue'
import ErrorState from '@/components/ErrorState.vue'

const props = defineProps<{ sessionId: string }>()
const router = useRouter()
const session = useSessionStore()
const profile = useProfileStore()
const config = useConfigStore()
const { showToast } = useToast()

const loadError = ref('')
const remaining = ref<number | null>(null)
const expired = ref(false)
const warned = ref(false)

let timerId: ReturnType<typeof setInterval> | null = null

const answers = computed(() => session.answeredCount)
const correct = computed(() => session.correctCount)
const wrong = computed(() => Math.max(0, answers.value - correct.value))
const accuracy = computed(() => session.accuracy)
const finished = computed(() => session.isCompleted)
const modeLabel = computed(() => {
  const mode = session.currentSession?.mode
  if (mode === 'EXAM') return t('quiz.modeExam')
  if (mode === 'CHAPTER') return t('quiz.modeChapter')
  return t('quiz.modePractice')
})

const correctLabel = computed(() => {
  const optionId = session.correctOptionId
  if (!optionId) return ''
  const option = session.currentQuestion?.content.options.find((item) => item.id === optionId)
  return option?.label ?? ''
})

function formatClock(totalSeconds: number): string {
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  return `${minutes}:${seconds.toString().padStart(2, '0')}`
}

function stopTimer(): void {
  if (timerId !== null) {
    clearInterval(timerId)
    timerId = null
  }
}

function startTimer(): void {
  stopTimer()
  const current = session.currentSession
  remaining.value = null
  expired.value = false
  warned.value = false
  if (!current || current.mode !== 'EXAM') return
  const startedAtMs = Date.parse(current.startedAt)
  if (!Number.isFinite(startedAtMs)) return
  const durationSec = config.examDurationSec
  const warningSec = config.timerWarningSec

  const tick = (): void => {
    const left = computeRemainingSeconds(startedAtMs, durationSec, Date.now())
    remaining.value = left
    if (left === 0 && !expired.value) {
      expired.value = true
      stopTimer()
      showToast(t('quiz.timerUp'), 'warning')
    } else if (left > 0 && left <= warningSec && !warned.value) {
      warned.value = true
      showToast(t('quiz.timerWarning', { seconds: left }), 'warning')
    }
  }

  tick()
  if (!expired.value) {
    timerId = setInterval(tick, 1000)
  }
}

async function load(): Promise<void> {
  loadError.value = ''
  try {
    await session.loadSession(props.sessionId)
    startTimer()
  } catch {
    loadError.value = t('quiz.sessionError')
  }
}

function selectOption(optionId: string): void {
  if (session.submitting || session.answeredCurrent || expired.value || finished.value) return
  session.selectedAnswer = optionId
}

async function submit(): Promise<void> {
  if (!session.selectedAnswer) {
    showToast(t('quiz.selectOption'), 'warning')
    return
  }
  if (session.submitting || session.answeredCurrent || expired.value) return
  try {
    await session.submitAnswer(session.selectedAnswer)
  } catch {
    showToast(t('quiz.submitError'), 'error')
  }
}

async function next(): Promise<void> {
  stopTimer()
  if (session.isLastQuestion) {
    session.finishSession()
    // Refresh mastery right away so returning home/profile shows the new data
    // without needing a manual page reload.
    void profile.loadSummary()
    return
  }
  await session.nextQuestion()
  startTimer()
}

function optionClass(optionId: string): Record<string, boolean> {
  const answered = session.answeredCurrent
  const isCorrect = session.correctOptionId === optionId
  const isSelected = session.selectedAnswer === optionId
  return {
    'option--selected': isSelected && !answered,
    'option--correct': answered && isCorrect,
    'option--wrong': answered && isSelected && !isCorrect,
    'option--muted': answered && !isSelected && !isCorrect
  }
}

function backHome(): void {
  stopTimer()
  void router.push({ name: 'home' })
}

function practiceAgain(): void {
  stopTimer()
  void router.push({ name: 'practice' })
}

onMounted(load)
onUnmounted(stopTimer)
</script>

<template>
  <div class="quiz-run">
    <header class="run-head">
      <div>
        <p class="eyebrow">{{ modeLabel }}</p>
        <h1 class="page-title">
          {{ t('quiz.runSheet') }}
          <small v-if="session.currentSession" class="run-index mono">
            {{ t('quiz.questionOf', { index: session.questionNumber, total: session.currentSession.totalQuestions }) }}
          </small>
        </h1>
      </div>
      <div class="run-meters">
        <div v-if="remaining !== null" class="meter-block" :class="{ 'meter-block--warning': remaining <= config.timerWarningSec }">
          <span class="label">{{ t('quiz.timer') }}</span>
          <span class="meter-value mono">{{ formatClock(remaining) }}</span>
        </div>
        <div class="meter-block">
          <span class="label">{{ t('quiz.progress') }}</span>
          <span class="meter-value mono">{{ session.progress }}%</span>
        </div>
      </div>
    </header>

    <div class="progress-track" role="progressbar" :aria-valuenow="session.progress" aria-valuemin="0" aria-valuemax="100">
      <div class="progress-fill" :style="{ width: `${session.progress}%` }" />
    </div>

    <LoadingSkeleton v-if="session.loading" :lines="6" />
    <ErrorState v-else-if="loadError" :message="loadError" @retry="load" />
    <ErrorState v-else-if="!session.currentSession && !finished" :message="t('quiz.sessionError')" @retry="load" />

    <section v-else-if="finished" class="summary">
      <header class="summary-head">
        <h2>{{ t('quiz.completeTitle') }}</h2>
        <p class="page-desc">{{ t('quiz.completeDesc') }}</p>
      </header>
      <div class="summary-grid">
        <div class="summary-figure">
          <span class="label">{{ t('quiz.answered') }}</span>
          <span class="figure">{{ answers }}</span>
        </div>
        <div class="summary-figure">
          <span class="label">{{ t('quiz.correctCount') }}</span>
          <span class="figure">{{ correct }}</span>
        </div>
        <div class="summary-figure">
          <span class="label">{{ t('quiz.wrongCount') }}</span>
          <span class="figure">{{ wrong }}</span>
        </div>
        <div class="summary-figure">
          <span class="label">{{ t('quiz.accuracy') }}</span>
          <span class="figure">{{ accuracy }}%</span>
        </div>
      </div>
      <footer class="summary-actions">
        <button class="btn btn-accent" @click="practiceAgain">{{ t('quiz.again') }}</button>
        <button class="btn btn-secondary" @click="backHome">{{ t('quiz.backHome') }}</button>
      </footer>
    </section>

    <article v-else-if="session.currentQuestion" class="question">
      <p class="stem">{{ session.currentQuestion.content.stem }}</p>

      <img
        v-if="session.currentQuestion.content.imageUrl"
        :src="session.currentQuestion.content.imageUrl"
        :alt="t('quiz.imageAlt')"
        class="question-image"
      />

      <div class="options" role="list">
        <button
          v-for="option in session.currentQuestion.content.options"
          :key="option.id"
          type="button"
          role="listitem"
          class="option"
          :class="optionClass(option.id)"
          :disabled="session.submitting || session.answeredCurrent || expired"
          @click="selectOption(option.id)"
        >
          <span class="option-label mono">{{ option.label }}</span>
          <span class="option-text">{{ option.text }}</span>
        </button>
      </div>

      <section v-if="session.answeredCurrent" class="verdict" :class="session.isCorrect ? 'verdict--correct' : 'verdict--wrong'">
        <p class="verdict-title">
          {{ session.isCorrect ? t('quiz.correct') : t('quiz.wrong') }}
        </p>
        <p v-if="correctLabel" class="verdict-line mono">
          {{ t('quiz.correctAnswer', { label: correctLabel }) }}
        </p>
        <p class="verdict-explanation">
          <span class="label">{{ t('quiz.explanation') }}</span>
          {{ session.explanation || t('quiz.noExplanation') }}
        </p>
      </section>

      <p v-if="expired" class="notice notice--warning" role="status">{{ t('quiz.timerUp') }}</p>

      <footer class="run-actions">
        <span v-if="session.answeredCurrent" class="lock-note">{{ t('quiz.answerLocked') }}</span>
        <button
          v-if="!session.answeredCurrent"
          class="btn btn-accent"
          :disabled="!session.selectedAnswer || session.submitting || expired"
          @click="submit"
        >
          {{ session.submitting ? t('quiz.submitting') : t('quiz.submit') }}
        </button>
        <button v-else class="btn btn-secondary" @click="next">
          {{ session.isLastQuestion ? t('quiz.finish') : t('quiz.next') }}
        </button>
      </footer>
    </article>

    <section v-else-if="expired" class="summary">
      <h2>{{ t('quiz.completeTitle') }}</h2>
      <p class="page-desc">{{ t('quiz.timedOut') }}</p>
      <footer class="summary-actions">
        <RouterLink to="/practice" class="btn btn-accent">{{ t('quiz.again') }}</RouterLink>
        <RouterLink to="/" class="btn btn-secondary">{{ t('quiz.backHome') }}</RouterLink>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.quiz-run {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  max-width: 56rem;
}

.run-head {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  justify-content: space-between;
  gap: var(--space-4);
  padding-bottom: var(--space-3);
  border-bottom: 2px solid var(--color-text);
}

.run-index {
  margin-left: var(--space-3);
  font-size: var(--text-xs);
  letter-spacing: 0.06em;
  color: var(--color-text-muted);
}

.run-meters {
  display: flex;
  gap: var(--space-3);
}

.meter-block {
  display: flex;
  flex-direction: column;
  min-width: 6rem;
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-rule);
  border-radius: var(--radius-xs);
  background-color: var(--color-surface);
}

.meter-block--warning {
  border-color: var(--color-warning);
}

.meter-value {
  font-size: var(--text-xl);
  line-height: 1.2;
}

.meter-block--warning .meter-value {
  color: var(--color-warning);
}

.progress-track {
  height: 4px;
  background-color: var(--color-bg-deep);
  border-radius: var(--radius-xs);
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background-color: var(--color-text);
}

.question {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.stem {
  margin: 0;
  font-family: var(--font-display);
  font-size: var(--text-2xl);
  font-weight: 600;
  line-height: 1.4;
}

.question-image {
  max-height: 22rem;
  object-fit: contain;
  border: 1px solid var(--color-rule);
  border-radius: var(--radius-xs);
  background-color: var(--color-surface);
}

.options {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.option {
  display: grid;
  grid-template-columns: 2rem minmax(0, 1fr);
  align-items: start;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border: 1px solid var(--color-rule-strong);
  border-radius: var(--radius-xs);
  background-color: var(--color-surface);
  text-align: left;
  color: var(--color-text);
}

.option:hover:not(:disabled) {
  border-color: var(--color-text);
}

.option--selected {
  border-color: var(--color-accent);
}

.option--selected .option-label {
  color: var(--color-accent-deep);
}

.option--correct {
  border-color: var(--color-correct);
  background-color: var(--color-correct-bg);
}

.option--wrong {
  border-color: var(--color-wrong);
  background-color: var(--color-wrong-bg);
}

.option--muted {
  opacity: 0.72;
}

.option-label {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 1.6rem;
  border: 1px solid currentColor;
  border-radius: var(--radius-xs);
  font-size: var(--text-xs);
}

.option-text {
  font-size: var(--text-base);
  line-height: 1.5;
}

.verdict {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-4);
  border: 1px solid var(--color-rule);
  border-radius: var(--radius-xs);
  background-color: var(--color-surface);
}

.verdict--correct {
  border-color: var(--color-correct);
}

.verdict--wrong {
  border-color: var(--color-wrong);
}

.verdict-title {
  margin: 0;
  font-family: var(--font-display);
  font-size: var(--text-lg);
  font-weight: 600;
}

.verdict--correct .verdict-title {
  color: var(--color-correct);
}

.verdict--wrong .verdict-title {
  color: var(--color-wrong);
}

.verdict-line {
  margin: 0;
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}

.verdict-explanation {
  margin: 0;
  padding-top: var(--space-2);
  border-top: 1px solid var(--color-rule);
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}

.run-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--space-3);
  padding-top: var(--space-3);
  border-top: 1px solid var(--color-rule);
}

.lock-note {
  margin-right: auto;
  font-size: var(--text-xs);
  color: var(--color-text-muted);
}

.summary {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  padding: var(--space-6);
  border: 1px solid var(--color-rule);
  border-radius: var(--radius-sm);
  background-color: var(--color-surface);
}

.summary-head h2 {
  margin: 0;
}

.summary-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  border-top: 1px solid var(--color-rule);
  border-bottom: 1px solid var(--color-rule);
}

.summary-figure {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  padding: var(--space-4) var(--space-3);
  border-right: 1px solid var(--color-rule);
}

.summary-figure:last-child {
  border-right: none;
}

.figure {
  font-family: var(--font-display);
  font-size: var(--text-3xl);
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.summary-actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

@media (max-width: 720px) {
  .summary-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .summary-figure:nth-child(2) {
    border-right: none;
  }

  .summary-figure:nth-child(-n + 2) {
    border-bottom: 1px solid var(--color-rule);
  }

  .run-actions {
    flex-direction: column-reverse;
    align-items: stretch;
  }

  .lock-note {
    margin-right: 0;
  }

  .run-actions .btn {
    width: 100%;
  }
}
</style>
