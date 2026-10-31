import type {
  BattleAnswerResult,
  BattleGameOver,
  BattleMode,
  BattleQuestionFrame,
  BattleRoomState
} from '@/types'

/**
 * Battle transport contract. The gateway accepts the access token in the query
 * string because the browser WebSocket API cannot set custom headers.
 */
export const BATTLE_WS_BASE = import.meta.env.VITE_BATTLE_WS_URL ?? '/ws/battle'

export const BATTLE_FRAME = {
  FIND_MATCH: 'find_match',
  CANCEL_MATCH: 'cancel_match',
  MATCH_FOUND: 'match_found',
  JOIN_ROOM: 'join_room',
  ROOM_STATE: 'room_state',
  QUESTION: 'question',
  ANSWER: 'answer',
  ANSWER_RESULT: 'answer_result',
  GAME_OVER: 'game_over',
  TOKEN_REFRESH_REQUIRED: 'token_refresh_required',
  TOKEN_UPDATE: 'token_update',
  REVOKED: 'revoked',
  ERROR: 'error'
} as const

export interface FindMatchFrame {
  type: typeof BATTLE_FRAME.FIND_MATCH
  mode: BattleMode
}

export interface CancelMatchFrame {
  type: typeof BATTLE_FRAME.CANCEL_MATCH
}

export interface JoinRoomFrame {
  type: typeof BATTLE_FRAME.JOIN_ROOM
  roomId: string
}

export interface AnswerFrame {
  type: typeof BATTLE_FRAME.ANSWER
  questionIndex: number
  optionId: string
}

export interface TokenUpdateFrame {
  type: typeof BATTLE_FRAME.TOKEN_UPDATE
  token: string
}

export type BattleClientFrame =
  | FindMatchFrame
  | CancelMatchFrame
  | JoinRoomFrame
  | AnswerFrame
  | TokenUpdateFrame

export interface MatchFoundFrame {
  type: typeof BATTLE_FRAME.MATCH_FOUND
  roomId: string
}

export interface RoomStateFrame extends BattleRoomState {
  type: typeof BATTLE_FRAME.ROOM_STATE
}

export interface QuestionFrame extends BattleQuestionFrame {
  type: typeof BATTLE_FRAME.QUESTION
}

export interface AnswerResultFrame extends BattleAnswerResult {
  type: typeof BATTLE_FRAME.ANSWER_RESULT
}

export interface GameOverFrame extends BattleGameOver {
  type: typeof BATTLE_FRAME.GAME_OVER
}

export interface ErrorFrame {
  type: typeof BATTLE_FRAME.ERROR
  message: string
}

export interface RevokedFrame {
  type: typeof BATTLE_FRAME.REVOKED
}

export interface TokenRefreshRequiredFrame {
  type: typeof BATTLE_FRAME.TOKEN_REFRESH_REQUIRED
}

export type BattleServerFrame =
  | MatchFoundFrame
  | RoomStateFrame
  | QuestionFrame
  | AnswerResultFrame
  | GameOverFrame
  | ErrorFrame
  | RevokedFrame
  | TokenRefreshRequiredFrame

export function buildBattleSocketUrl(token: string, base: string = BATTLE_WS_BASE): string {
  const separator = base.includes('?') ? '&' : '?'
  return `${base}${separator}token=${encodeURIComponent(token)}`
}

export function findMatchFrame(mode: BattleMode): FindMatchFrame {
  return { type: BATTLE_FRAME.FIND_MATCH, mode }
}

export function cancelMatchFrame(): CancelMatchFrame {
  return { type: BATTLE_FRAME.CANCEL_MATCH }
}

export function joinRoomFrame(roomId: string): JoinRoomFrame {
  return { type: BATTLE_FRAME.JOIN_ROOM, roomId }
}

export function answerFrame(questionIndex: number, optionId: string): AnswerFrame {
  return { type: BATTLE_FRAME.ANSWER, questionIndex, optionId }
}

export function tokenUpdateFrame(token: string): TokenUpdateFrame {
  return { type: BATTLE_FRAME.TOKEN_UPDATE, token }
}
