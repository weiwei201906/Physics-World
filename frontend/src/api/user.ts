import { gqlRequest } from './graphql'
import type {
  User,
  QuizSession,
  Question,
  SubmitAttemptInput,
  SubmitAttemptPayload,
  LeaderboardPage,
  LeaderboardPeriod
} from '@/types'

const ME_QUERY = `
  query Me {
    me { id email nickname avatarUrl roles }
  }
`

const START_SESSION_MUTATION = `
  mutation StartQuizSession($mode: QuizMode!, $chapterId: ID) {
    startQuizSession(mode: $mode, chapterId: $chapterId) {
      id mode chapterId status currentIndex totalQuestions startedAt
    }
  }
`

const GET_SESSION_QUERY = `
  query QuizSession($id: ID!) {
    quizSession(id: $id) {
      id mode chapterId status currentIndex totalQuestions startedAt
    }
  }
`

const GET_QUESTION_QUERY = `
  query SessionQuestion($sessionId: ID!, $index: Int!) {
    sessionQuestion(sessionId: $sessionId, index: $index) {
      id chapterId difficulty
      content { stem imageUrl explanation options { id label text } }
    }
  }
`

const SUBMIT_ATTEMPT_MUTATION = `
  mutation SubmitAttempt($input: SubmitAttemptInput!) {
    submitAttempt(input: $input) { accepted attemptId }
  }
`

const LEADERBOARD_QUERY = `
  query Leaderboard($period: Period!, $page: Int!, $size: Int!) {
    leaderboard(period: $period, page: $page, size: $size) {
      entries { rank userId nickname avatarUrl score }
      period page pageSize total
    }
  }
`

export async function fetchMe(): Promise<User> {
  return gqlRequest<{ me: User }>(ME_QUERY).then((d) => d.me)
}

export async function startQuizSession(
  mode: 'PRACTICE' | 'EXAM' | 'CHAPTER',
  chapterId?: string
): Promise<QuizSession> {
  return gqlRequest<{ startQuizSession: QuizSession }>(START_SESSION_MUTATION, {
    mode,
    chapterId: chapterId ?? null
  }).then((d) => d.startQuizSession)
}

export async function fetchQuizSession(id: string): Promise<QuizSession> {
  return gqlRequest<{ quizSession: QuizSession }>(GET_SESSION_QUERY, { id }).then(
    (d) => d.quizSession
  )
}

export async function fetchSessionQuestion(
  sessionId: string,
  index: number
): Promise<Question> {
  return gqlRequest<{ sessionQuestion: Question }>(GET_QUESTION_QUERY, {
    sessionId,
    index
  }).then((d) => d.sessionQuestion)
}

export async function submitAttempt(input: SubmitAttemptInput): Promise<SubmitAttemptPayload> {
  return gqlRequest<{ submitAttempt: SubmitAttemptPayload }>(SUBMIT_ATTEMPT_MUTATION, {
    input
  }).then((d) => d.submitAttempt)
}

export async function fetchLeaderboard(
  period: LeaderboardPeriod,
  page: number,
  size: number
): Promise<LeaderboardPage> {
  return gqlRequest<{ leaderboard: LeaderboardPage }>(LEADERBOARD_QUERY, {
    period,
    page,
    size
  }).then((d) => d.leaderboard)
}
