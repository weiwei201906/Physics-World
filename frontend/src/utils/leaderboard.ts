import type { LeaderboardEntry } from '@/types'

export interface LeaderboardRow extends LeaderboardEntry {
  isSelf: boolean
}

/**
 * Map the server page into display rows, flagging the signed-in user. The
 * server is authoritative for rank and score; no client-side reordering.
 */
export function mapLeaderboardRows(
  entries: LeaderboardEntry[],
  currentUserId: string | null
): LeaderboardRow[] {
  return entries.map((entry) => ({
    ...entry,
    isSelf: currentUserId !== null && entry.userId === currentUserId
  }))
}

export function leaderboardTotalPages(total: number, pageSize: number): number {
  if (pageSize <= 0 || total <= 0) return 1
  return Math.ceil(total / pageSize)
}

export function clampLeaderboardPage(page: number, total: number, pageSize: number): number {
  const last = leaderboardTotalPages(total, pageSize)
  if (!Number.isFinite(page)) return 1
  return Math.max(1, Math.min(last, Math.floor(page)))
}
