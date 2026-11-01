import { gqlRequest } from './graphql'
import type {
  User,
  Chapter,
  QuizSession,
  QuizMode,
  Question,
  SubmitAttemptInput,
  SubmitAttemptPayload,
  LeaderboardPage,
  LeaderboardPeriod,
  RankSummary,
  UpdateProfileInput
} from '@/types'

const USER_FIELDS = 'id email nickname avatarUrl roles createdAt'
const SESSION_FIELDS =
  'id mode chapterId status currentIndex totalQuestions startedAt completedAt'
const QUESTION_FIELDS = `
  id chapterId difficulty
  content { stem imageUrl explanation options { id label text } }
`

const ME_QUERY = `
  query Me {
    me { ${USER_FIELDS} }
  }
`

const CHAPTERS_QUERY = `
  query Chapters {
    chapters { id name questionCount }
  }
`

const START_SESSION_MUTATION = `
  mutation StartQuizSession($mode: QuizMode!, $chapterId: ID, $questionIds: [ID!]) {
    startQuizSession(mode: $mode, chapterId: $chapterId, questionIds: $questionIds) {
      ${SESSION_FIELDS}
    }
  }
`

const GET_SESSION_QUERY = `
  query QuizSession($id: UUID!) {
    quizSession(id: $id) {
      ${SESSION_FIELDS}
    }
  }
`

const GET_QUESTION_QUERY = `
  query SessionQuestion($sessionId: UUID!, $index: Int!) {
    sessionQuestion(sessionId: $sessionId, index: $index) {
      ${QUESTION_FIELDS}
    }
  }
`

const SUBMIT_ATTEMPT_MUTATION = `
  mutation SubmitAttempt($input: SubmitAttemptInput!) {
    submitAttempt(input: $input) {
      accepted attemptId correct correctOptionId explanation
    }
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

const MY_RANK_QUERY = `
  query MyRank($period: Period!) {
    myRank(period: $period) { period rank score }
  }
`

const UPDATE_PROFILE_MUTATION = `
  mutation UpdateProfile($input: UpdateProfileInput!) {
    updateProfile(input: $input) { ${USER_FIELDS} }
  }
`

export async function fetchMe(signal?: AbortSignal): Promise<User> {
  return gqlRequest<{ me: User }>(ME_QUERY, undefined, signal).then((d) => d.me)
}

export async function fetchChapters(signal?: AbortSignal): Promise<Chapter[]> {
  return gqlRequest<{ chapters: Chapter[] }>(CHAPTERS_QUERY, undefined, signal).then(
    (d) => d.chapters
  )
}

export async function startQuizSession(
  mode: QuizMode,
  chapterId?: string,
  questionIds?: string[],
  signal?: AbortSignal
): Promise<QuizSession> {
  return gqlRequest<{ startQuizSession: QuizSession }>(
    START_SESSION_MUTATION,
    { mode, chapterId: chapterId ?? null, questionIds: questionIds ?? null },
    signal
  ).then((d) => d.startQuizSession)
}

export async function fetchQuizSession(
  id: string,
  signal?: AbortSignal
): Promise<QuizSession | null> {
  return gqlRequest<{ quizSession: QuizSession | null }>(
    GET_SESSION_QUERY,
    { id },
    signal
  ).then((d) => d.quizSession)
}

export async function fetchSessionQuestion(
  sessionId: string,
  index: number,
  signal?: AbortSignal
): Promise<Question | null> {
  return gqlRequest<{ sessionQuestion: Question | null }>(
    GET_QUESTION_QUERY,
    { sessionId, index },
    signal
  ).then((d) => d.sessionQuestion)
}

export async function submitAttempt(
  input: SubmitAttemptInput,
  signal?: AbortSignal
): Promise<SubmitAttemptPayload> {
  return gqlRequest<{ submitAttempt: SubmitAttemptPayload }>(
    SUBMIT_ATTEMPT_MUTATION,
    { input },
    signal
  ).then((d) => d.submitAttempt)
}

export async function fetchLeaderboard(
  period: LeaderboardPeriod,
  page: number,
  size: number,
  signal?: AbortSignal
): Promise<LeaderboardPage> {
  return gqlRequest<{ leaderboard: LeaderboardPage }>(
    LEADERBOARD_QUERY,
    { period, page, size },
    signal
  ).then((d) => d.leaderboard)
}

export async function fetchMyRank(
  period: LeaderboardPeriod,
  signal?: AbortSignal
): Promise<RankSummary> {
  return gqlRequest<{ myRank: RankSummary }>(MY_RANK_QUERY, { period }, signal).then(
    (d) => d.myRank
  )
}

export async function updateProfile(
  input: UpdateProfileInput,
  signal?: AbortSignal
): Promise<User> {
  return gqlRequest<{ updateProfile: User }>(
    UPDATE_PROFILE_MUTATION,
    { input },
    signal
  ).then((d) => d.updateProfile)
}
