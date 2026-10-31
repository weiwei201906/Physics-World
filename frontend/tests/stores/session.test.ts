import { beforeEach, describe, expect, it, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useSessionStore } from '@/stores/session'

vi.mock('@/api/user', () => ({
  startQuizSession: vi.fn(),
  fetchQuizSession: vi.fn(),
  fetchSessionQuestion: vi.fn(),
  submitAttempt: vi.fn()
}))

import { startQuizSession, fetchQuizSession, fetchSessionQuestion, submitAttempt } from '@/api/user'

describe('session store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('starts a session and loads the first question', async () => {
    vi.mocked(startQuizSession).mockResolvedValue({
      id: 's1',
      mode: 'PRACTICE',
      chapterId: null,
      status: 'ACTIVE',
      currentIndex: 0,
      totalQuestions: 5,
      startedAt: '2026-01-01T00:00:00Z',
      completedAt: null
    })
    vi.mocked(fetchSessionQuestion).mockResolvedValue({
      id: 'q1',
      chapterId: 'c1',
      difficulty: 1,
      content: { stem: 'Q?', imageUrl: null, explanation: null, options: [{ id: 'a', label: 'A', text: 'opt a' }] }
    })

    const store = useSessionStore()
    const id = await store.startSession('PRACTICE')

    expect(id).toBe('s1')
    expect(store.currentSession?.id).toBe('s1')
    expect(store.currentQuestion?.id).toBe('q1')
    expect(store.progress).toBe(20)
  })

  it('marks last question correctly', async () => {
    vi.mocked(startQuizSession).mockResolvedValue({
      id: 's1',
      mode: 'PRACTICE',
      chapterId: null,
      status: 'ACTIVE',
      currentIndex: 0,
      totalQuestions: 1,
      startedAt: '2026-01-01T00:00:00Z',
      completedAt: null
    })
    vi.mocked(fetchSessionQuestion).mockResolvedValue({
      id: 'q1',
      chapterId: 'c1',
      difficulty: 1,
      content: { stem: 'Q?', imageUrl: null, explanation: null, options: [] }
    })

    const store = useSessionStore()
    await store.startSession('PRACTICE')
    expect(store.isLastQuestion).toBe(true)
  })

  it('submitAnswer records selected answer and calls api', async () => {
    vi.mocked(startQuizSession).mockResolvedValue({
      id: 's1',
      mode: 'PRACTICE',
      chapterId: null,
      status: 'ACTIVE',
      currentIndex: 0,
      totalQuestions: 5,
      startedAt: '2026-01-01T00:00:00Z',
      completedAt: null
    })
    vi.mocked(fetchSessionQuestion).mockResolvedValue({
      id: 'q1',
      chapterId: 'c1',
      difficulty: 1,
      content: { stem: 'Q?', imageUrl: null, explanation: null, options: [{ id: 'a', label: 'A', text: 'a' }] }
    })
    vi.mocked(submitAttempt).mockResolvedValue({
      accepted: true,
      attemptId: 'att1',
      correct: true,
      correctOptionId: 'a',
      explanation: null
    })

    const store = useSessionStore()
    await store.startSession('PRACTICE')
    await store.submitAnswer('a')

    expect(store.selectedAnswer).toBe('a')
    expect(submitAttempt).toHaveBeenCalledWith({
      sessionId: 's1',
      questionId: 'q1',
      answer: 'a'
    })
  })

  it('nextQuestion advances index and loads next question', async () => {
    vi.mocked(startQuizSession).mockResolvedValue({
      id: 's1',
      mode: 'PRACTICE',
      chapterId: null,
      status: 'ACTIVE',
      currentIndex: 0,
      totalQuestions: 3,
      startedAt: '2026-01-01T00:00:00Z',
      completedAt: null
    })
    vi.mocked(fetchSessionQuestion).mockResolvedValue({
      id: 'q1',
      chapterId: 'c1',
      difficulty: 1,
      content: { stem: 'Q?', imageUrl: null, explanation: null, options: [] }
    })

    const store = useSessionStore()
    await store.startSession('PRACTICE')
    await store.nextQuestion()

    expect(store.currentSession?.currentIndex).toBe(1)
    expect(fetchSessionQuestion).toHaveBeenCalledWith('s1', 1)
  })

  it('reset clears session state', async () => {
    vi.mocked(startQuizSession).mockResolvedValue({
      id: 's1',
      mode: 'PRACTICE',
      chapterId: null,
      status: 'ACTIVE',
      currentIndex: 0,
      totalQuestions: 3,
      startedAt: '2026-01-01T00:00:00Z',
      completedAt: null
    })
    vi.mocked(fetchSessionQuestion).mockResolvedValue({
      id: 'q1',
      chapterId: 'c1',
      difficulty: 1,
      content: { stem: 'Q?', imageUrl: null, explanation: null, options: [] }
    })

    const store = useSessionStore()
    await store.startSession('PRACTICE')
    store.reset()

    expect(store.currentSession).toBeNull()
    expect(store.currentQuestion).toBeNull()
  })

  it('loadSession fetches existing session by id', async () => {
    vi.mocked(fetchQuizSession).mockResolvedValue({
      id: 's2',
      mode: 'CHAPTER',
      chapterId: 'c1',
      status: 'ACTIVE',
      currentIndex: 2,
      totalQuestions: 10,
      startedAt: '2026-01-01T00:00:00Z',
      completedAt: null
    })
    vi.mocked(fetchSessionQuestion).mockResolvedValue({
      id: 'q3',
      chapterId: 'c1',
      difficulty: 1,
      content: { stem: 'Q?', imageUrl: null, explanation: null, options: [] }
    })

    const store = useSessionStore()
    await store.loadSession('s2')

    expect(store.currentSession?.id).toBe('s2')
    expect(fetchSessionQuestion).toHaveBeenCalledWith('s2', 2)
  })
})
