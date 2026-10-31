// Domain types shared across the frontend, aligned with contracts/graphql.

export interface User {
  id: string
  email: string
  nickname: string
  avatarUrl: string | null
  roles: string[]
  createdAt: string
}

export interface Chapter {
  id: string
  name: string
  questionCount: number
}

export type QuizMode = 'PRACTICE' | 'EXAM' | 'CHAPTER'

export type SessionStatus = 'ACTIVE' | 'COMPLETED' | 'ABANDONED'

export interface QuizSession {
  id: string
  mode: QuizMode
  chapterId: string | null
  status: SessionStatus
  currentIndex: number
  totalQuestions: number
  startedAt: string
  completedAt: string | null
}

export type LeaderboardPeriod = 'DAILY' | 'WEEKLY' | 'ALL'

export interface QuestionOption {
  id: string
  label: string
  text: string
}

export interface QuestionContent {
  stem: string
  imageUrl: string | null
  options: QuestionOption[]
  explanation: string | null
}

export interface Question {
  id: string
  chapterId: string
  difficulty: number
  content: QuestionContent
}

export interface SubmitAttemptInput {
  sessionId: string
  questionId: string
  answer: string
}

export interface SubmitAttemptPayload {
  accepted: boolean
  attemptId: string
  correct: boolean
  correctOptionId: string | null
  explanation: string | null
}

export interface UpdateProfileInput {
  nickname?: string
  avatarUrl?: string
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

export interface RankSummary {
  period: LeaderboardPeriod
  rank: number | null
  score: number
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

export interface ConfigEntry {
  namespace: string
  key: string
  value: string
  updatedAt: string
}

export interface ConfigMutationPayload {
  applied: boolean
  degraded: boolean
  entry: ConfigEntry | null
  errorMessage: string | null
}

export interface ConfigSchemaEntry {
  namespace: string
  key: string
  type: string
  required: boolean
  defaultValue: string | null
  description: string | null
}

export interface ConfigAuditEntry {
  id: string
  namespace: string
  key: string
  oldValue: string | null
  newValue: string | null
  changedBy: string | null
  changedAt: string
}

export interface QuestionVersion {
  id: string
  questionId: string
  version: number
  deletedAt: string | null
  createdAt: string
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
    examDurationSec: number
  }
  maintenance: {
    enabled: boolean
    message: string
  }
  pollIntervalSec: number
  auth: {
    preRefreshSec: number
    silentRestoreTimeoutSec: number
  }
}

export type BattleMode = '1v1' | 'squad'

export type BattleStatus = 'waiting' | 'matching' | 'playing' | 'settling' | 'finished'

export interface BattlePlayer {
  userId: string
  nickname: string
  score: number
  rank: number
  correctCount: number
  connected: boolean
}

export interface BattleRoomState {
  roomId: string
  status: BattleStatus
  players: BattlePlayer[]
  currentIndex: number
  totalQuestions: number
}

export interface BattleQuestionBody {
  id: string
  stem: string
  options: Array<{ id: string; text: string }>
}

export interface BattleQuestionFrame {
  index: number
  total: number
  seconds: number | null
  question: BattleQuestionBody
}

export interface BattleAnswerResult {
  index: number
  correct: boolean
  correctOptionId: string | null
  score: number
  totalScore: number
  explanation: string | null
}

export interface BattleGameOver {
  winnerId: string | null
  players: BattlePlayer[]
  finalScore: number
}

export type BattleResultTag = 'win' | 'lose' | 'draw'

export interface BattleHistoryEntry {
  battleId: string
  mode: string
  questionCount: number
  startedAt: string
  finishedAt: string
  myScore: number
  myCorrect: number
  myRank: number
  opponentId: string | null
  opponentNickname: string | null
  opponentScore: number | null
  opponentCorrect: number | null
  result: BattleResultTag
}
