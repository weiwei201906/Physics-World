import { recGqlRequest } from './graphql'
import type { Question, ProfileSummary } from '@/types'

const RECOMMEND_QUERY = `
  query RecommendQuestions($chapterId: ID, $count: Int) {
    recommendQuestions(chapterId: $chapterId, count: $count) {
      id chapterId difficulty
      content { stem imageUrl explanation options { id label text } }
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
  count?: number
): Promise<Question[]> {
  return recGqlRequest<{ recommendQuestions: Question[] }>(RECOMMEND_QUERY, {
    chapterId: chapterId ?? null,
    count: count ?? null
  }).then((d) => d.recommendQuestions)
}

export async function fetchProfileSummary(): Promise<ProfileSummary> {
  return recGqlRequest<{ userProfileSummary: ProfileSummary }>(PROFILE_SUMMARY_QUERY).then(
    (d) => d.userProfileSummary
  )
}
