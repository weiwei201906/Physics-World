import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { QuizSession, Question, QuizMode, SubmitAttemptPayload } from '@/types'
import {
  startQuizSession as apiStartSession,
  fetchQuizSession as apiFetchSession,
  fetchSessionQuestion as apiFetchQuestion,
  submitAttempt as apiSubmitAttempt
} from '@/api/user'

export const useSessionStore = defineStore('session', () => {
  const currentSession = ref<QuizSession | null>(null)
  const currentQuestion = ref<Question | null>(null)
  const selectedAnswer = ref<string | null>(null)
  const isCorrect = ref<boolean | null>(null)
  const correctOptionId = ref<string | null>(null)
  const explanation = ref<string | null>(null)
  const lastResult = ref<SubmitAttemptPayload | null>(null)
  const answeredCurrent = ref(false)
  const loading = ref(false)
  const submitting = ref(false)
  const isCompleted = ref(false)
  const answeredCount = ref(0)
  const correctCount = ref(0)

  const isLastQuestion = computed(() => {
    if (!currentSession.value) return true
    return currentSession.value.currentIndex >= currentSession.value.totalQuestions - 1
  })

  const progress = computed(() => {
    if (!currentSession.value || currentSession.value.totalQuestions === 0) return 0
    return Math.min(
      100,
      Math.round(
        ((currentSession.value.currentIndex + 1) / currentSession.value.totalQuestions) * 100
      )
    )
  })

  const questionNumber = computed(() => (currentSession.value?.currentIndex ?? 0) + 1)
  const accuracy = computed(() =>
    answeredCount.value === 0 ? 0 : Math.round((correctCount.value / answeredCount.value) * 100)
  )
  const mode = computed<QuizMode | null>(() => currentSession.value?.mode ?? null)

  function resetPerQuestion(): void {
    selectedAnswer.value = null
    isCorrect.value = null
    correctOptionId.value = null
    explanation.value = null
    lastResult.value = null
    answeredCurrent.value = false
  }

  function resetSessionState(): void {
    currentSession.value = null
    currentQuestion.value = null
    isCompleted.value = false
    answeredCount.value = 0
    correctCount.value = 0
    resetPerQuestion()
  }

  async function loadQuestion(index: number): Promise<void> {
    if (!currentSession.value) return
    loading.value = true
    resetPerQuestion()
    try {
      currentQuestion.value = await apiFetchQuestion(currentSession.value.id, index)
    } finally {
      loading.value = false
    }
  }

  async function startSession(mode: QuizMode, chapterId?: string): Promise<string> {
    loading.value = true
    resetSessionState()
    try {
      const session = await apiStartSession(mode, chapterId)
      currentSession.value = session
      isCompleted.value = session.status === 'COMPLETED'
      await loadQuestion(session.currentIndex)
      return session.id
    } finally {
      loading.value = false
    }
  }

  async function loadSession(id: string): Promise<void> {
    loading.value = true
    resetSessionState()
    try {
      const session = await apiFetchSession(id)
      if (!session) {
        resetSessionState()
        return
      }
      currentSession.value = session
      isCompleted.value = session.status === 'COMPLETED'
      if (!isCompleted.value) {
        await loadQuestion(session.currentIndex)
      }
    } finally {
      loading.value = false
    }
  }

  /**
   * Submit exactly once per question. The returned payload is authoritative for
   * correctness, the correct option, and the explanation.
   */
  async function submitAnswer(answer: string): Promise<SubmitAttemptPayload | null> {
    if (!currentSession.value || !currentQuestion.value) return null
    if (submitting.value || answeredCurrent.value || isCompleted.value) return null

    submitting.value = true
    selectedAnswer.value = answer
    try {
      const result = await apiSubmitAttempt({
        sessionId: currentSession.value.id,
        questionId: currentQuestion.value.id,
        answer
      })
      lastResult.value = result
      isCorrect.value = result.correct
      correctOptionId.value = result.correctOptionId
      explanation.value = result.explanation ?? currentQuestion.value.content.explanation
      answeredCurrent.value = true
      answeredCount.value += 1
      if (result.correct) correctCount.value += 1
      return result
    } finally {
      submitting.value = false
    }
  }

  async function nextQuestion(): Promise<void> {
    if (!currentSession.value) return
    if (isLastQuestion.value) {
      currentSession.value.status = 'COMPLETED'
      currentSession.value.completedAt = new Date().toISOString()
      isCompleted.value = true
      return
    }
    currentSession.value.currentIndex += 1
    await loadQuestion(currentSession.value.currentIndex)
  }

  function finishSession(): void {
    if (!currentSession.value) return
    currentSession.value.status = 'COMPLETED'
    currentSession.value.completedAt = new Date().toISOString()
    isCompleted.value = true
  }

  function reset(): void {
    resetSessionState()
  }

  return {
    currentSession,
    currentQuestion,
    selectedAnswer,
    isCorrect,
    correctOptionId,
    explanation,
    lastResult,
    answeredCurrent,
    loading,
    submitting,
    isCompleted,
    answeredCount,
    correctCount,
    isLastQuestion,
    progress,
    questionNumber,
    accuracy,
    mode,
    startSession,
    loadSession,
    loadQuestion,
    submitAnswer,
    nextQuestion,
    finishSession,
    reset
  }
})
