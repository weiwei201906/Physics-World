// Domain types shared across the frontend.

export interface User {
  id: string
  email: string
  nickname: string
  avatarUrl: string | null
  roles: string[]
}

export type QuizMode = 'PRACTICE' | 'EXAM' | 'CHAPTER'

export type LeaderboardPeriod = 'DAILY' | 'WEEKLY' | 'ALL'

export interface QuestionOption {
  id: string
  label: string
  text: string
}

export interface QuestionContent {
  stem: string
  imageUrl?: string
  options: QuestionOption[]
  explanation?: string
}

export interface Question {
  id: string
  chapterId: string
  difficulty: number
  content: QuestionContent
}

export interface QuizSession {
  id: string
  mode: QuizMode
  chapterId: string | null
  status: 'ACTIVE' | 'COMPLETED' | 'ABANDONED'
  currentIndex: number
  totalQuestions: number
  startedAt: string
}

export interface LeaderboardEntry {
  rank: number
  userId: string
  nickname: string
  avatarUrl: string | null
  score: number
}

export interface LeaderboardPage {
  entries: LeaderboardEntry[]
  period: LeaderboardPeriod
  page: number
  pageSize: number
  total: number
}

export interface SubmitAttemptInput {
  sessionId: string
  questionId: string
  answer: string
}

export interface SubmitAttemptPayload {
  accepted: boolean
  attemptId: string
}

export interface ChapterStrength {
  chapterId: string
  chapterName: string
  strength: number
  questionCount: number
}

export interface ProfileSummary {
  userId: string
  totalAttempts: number
  correctRate: number
  chapterStrengths: ChapterStrength[]
}

export interface WebConfig {
  features: {
    battleEnabled: boolean
    recommendationEnabled: boolean
    sseEnabled: boolean
  }
  cdn: {
    mediaBaseUrl: string
  }
  ui: {
    theme: string
    pageSize: number
  }
  quiz: {
    timerWarningSec: number
  }
  maintenance: {
    enabled: boolean
    message: string
  }
  pollIntervalSec: number
}

export interface BattleRoom {
  id: string
  mode: string
  status: 'waiting' | 'matching' | 'playing' | 'settling' | 'finished'
  players: BattlePlayer[]
  currentQuestionIndex: number
}

export interface BattlePlayer {
  userId: string
  nickname: string
  score: number
  connected: boolean
}
