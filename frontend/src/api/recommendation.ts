import { recGqlRequest } from './graphql'
import type { Question, ProfileSummary } from '@/types'

const QUESTION_FIELDS = `
  id chapterId difficulty
  content { stem imageUrl explanation options { id label text } }
`

const RECOMMEND_QUERY = `
  query RecommendQuestions($chapterId: ID, $count: Int) {
    recommendQuestions(chapterId: $chapterId, count: $count) {
      ${QUESTION_FIELDS}
    }
  }
`

const PROFILE_SUMMARY_QUERY = `
  query UserProfileSummary {
    userProfileSummary {
      userId totalAttempts correctRate
      chapterStrengths { chapterId chapterName strength questionCount }
    }
  }
`

export async function fetchRecommendedQuestions(
  chapterId?: string,
  count?: number,
  signal?: AbortSignal
): Promise<Question[]> {
  return recGqlRequest<{ recommendQuestions: Question[] }>(
    RECOMMEND_QUERY,
    { chapterId: chapterId ?? null, count: count ?? null },
    signal
  ).then((d) => d.recommendQuestions)
}

export async function fetchProfileSummary(signal?: AbortSignal): Promise<ProfileSummary> {
  return recGqlRequest<{ userProfileSummary: ProfileSummary }>(
    PROFILE_SUMMARY_QUERY,
    undefined,
    signal
  ).then((d) => d.userProfileSummary)
}
