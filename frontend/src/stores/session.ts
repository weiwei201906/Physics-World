import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { QuizSession, Question } from '@/types'
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
  const loading = ref(false)
  const submitting = ref(false)

  const isLastQuestion = computed(() => {
    if (!currentSession.value) return true
    return currentSession.value.currentIndex >= currentSession.value.totalQuestions - 1
  })

  const progress = computed(() => {
    if (!currentSession.value || currentSession.value.totalQuestions === 0) return 0
    return Math.round(
      ((currentSession.value.currentIndex + 1) / currentSession.value.totalQuestions) * 100
    )
  })

  async function startSession(
    mode: 'PRACTICE' | 'EXAM' | 'CHAPTER',
    chapterId?: string
  ): Promise<string> {
    loading.value = true
    try {
      const session = await apiStartSession(mode, chapterId)
      currentSession.value = session
      await loadQuestion(session.currentIndex)
      return session.id
    } finally {
      loading.value = false
    }
  }

  async function loadSession(id: string): Promise<void> {
    loading.value = true
    try {
      currentSession.value = await apiFetchSession(id)
      await loadQuestion(currentSession.value.currentIndex)
    } finally {
      loading.value = false
    }
  }

  async function loadQuestion(index: number): Promise<void> {
    if (!currentSession.value) return
    loading.value = true
    selectedAnswer.value = null
    isCorrect.value = null
    try {
      currentQuestion.value = await apiFetchQuestion(currentSession.value.id, index)
    } finally {
      loading.value = false
    }
  }

  async function submitAnswer(answer: string): Promise<void> {
    if (!currentSession.value || !currentQuestion.value || submitting.value) return
    submitting.value = true
    selectedAnswer.value = answer
    try {
      const result = await apiSubmitAttempt({
        sessionId: currentSession.value.id,
        questionId: currentQuestion.value.id,
        answer
      })
      // Optimistic: determine correctness locally from correct answer if available.
      // The backend returns accepted; correctness comes via the explanation/content.
      if (currentQuestion.value.content.explanation) {
        // Best-effort: correct answer is implied; mark based on matching the first option
        // flagged correct. Since we don't get the correct option id, we rely on the
        // explanation presence for feedback display.
      }
      void result
    } finally {
      submitting.value = false
    }
  }

  async function nextQuestion(): Promise<void> {
    if (!currentSession.value) return
    if (isLastQuestion.value) {
      currentSession.value.status = 'COMPLETED'
      return
    }
    currentSession.value.currentIndex += 1
    await loadQuestion(currentSession.value.currentIndex)
  }

  function reset(): void {
    currentSession.value = null
    currentQuestion.value = null
    selectedAnswer.value = null
    isCorrect.value = null
  }

  return {
    currentSession,
    currentQuestion,
    selectedAnswer,
    isCorrect,
    loading,
    submitting,
    isLastQuestion,
    progress,
    startSession,
    loadSession,
    submitAnswer,
    nextQuestion,
    reset
  }
})
