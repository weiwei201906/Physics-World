import { computed, reactive, ref } from 'vue'
import {
  answerFrame,
  BATTLE_FRAME,
  type AnswerFrame,
  type BattleClientFrame
} from '@/api/battle'
import type {
  BattleAnswerResult,
  BattleGameOver,
  BattlePlayer,
  BattleQuestionFrame,
  BattleRoomState
} from '@/types'

/**
 * Client-side battle lifecycle. The state machine owns every piece of mutable
 * battle UI state and the only way to change it is through `transition(frame)`
 * or guarded actions (`submit`, `confirmEnd`) — no scattered
 * `if (msg.type === ...)` branches in the view.
 *
 *   waiting ──question──▶ answering ──answer_result──▶ revealed
 *                              ▲                            │
 *                              └──────── question ──────────┘
 *   waiting/answering ──game_over──▶ finished
 *   revealed ──game_over──▶ revealed (pendingFinish, shows "end battle" button)
 *   revealed (pendingFinish) ──confirmEnd──▶ finished
 *   any state ──room_state──▶ (updates players, no transition)
 *
 * The last question's answer result is shown in `revealed`; the server then
 * sends game_over but we hold the settlement until the player confirms, so
 * they actually see the final feedback before the report.
 */
export type BattleRoomPhase = 'waiting' | 'answering' | 'revealed' | 'finished'

export interface BattleQuestion {
  stem: string
  options: Array<{ id: string; text: string }>
}

export interface BattleRoomContext {
  players: BattlePlayer[]
  currentQuestion: BattleQuestion | null
  questionIndex: number
  selectedAnswer: string | null
  answerResult: BattleAnswerResult | null
  winnerId: string | null
  /** game_over arrived while showing the last answer result; awaiting confirm. */
  pendingFinish: boolean
}

type ServerFrame = Record<string, unknown>

type TransitionHandler = (frame: ServerFrame) => void

/**
 * Per-state transition table. A missing handler for a frame type means the
 * frame is ignored in that state (e.g. an answer_result arriving while we are
 * already finished is dropped).
 */
type TransitionTable = Record<BattleRoomPhase, Partial<Record<string, TransitionHandler>>>

export function useBattleRoomState(send: (frame: BattleClientFrame) => void) {
  const phase = ref<BattleRoomPhase>('waiting')

  const context = reactive<BattleRoomContext>({
    players: [],
    currentQuestion: null,
    questionIndex: 0,
    selectedAnswer: null,
    answerResult: null,
    winnerId: null,
    pendingFinish: false
  })

  function enterAnswering(frame: ServerFrame): void {
    const q = (frame as unknown as BattleQuestionFrame).question
    context.questionIndex = (frame as unknown as BattleQuestionFrame).index ?? context.questionIndex
    context.currentQuestion = q
      ? { stem: q.stem, options: q.options }
      : null
    context.selectedAnswer = null
    context.answerResult = null
    context.pendingFinish = false
    phase.value = 'answering'
  }

  function enterRevealed(frame: ServerFrame): void {
    context.answerResult = frame as unknown as BattleAnswerResult
    phase.value = 'revealed'
  }

  function enterFinished(frame: ServerFrame): void {
    const over = frame as unknown as BattleGameOver
    context.players = over.players ?? []
    context.winnerId = over.winnerId ?? null
    context.pendingFinish = false
    phase.value = 'finished'
  }

  /**
   * game_over arrived while the player is viewing the last question's answer
   * result. Stash the settlement data and flag `pendingFinish` so the view can
   * show an "end battle" button; the report is shown only after confirmEnd().
   */
  function holdFinish(frame: ServerFrame): void {
    const over = frame as unknown as BattleGameOver
    context.players = over.players ?? []
    context.winnerId = over.winnerId ?? null
    context.pendingFinish = true
  }

  function updatePlayers(frame: ServerFrame): void {
    const state = frame as unknown as BattleRoomState
    context.players = state.players ?? []
    // The backend status is informational; the client phase is driven by
    // question / answer_result / game_over frames.
  }

  const transitions: TransitionTable = {
    waiting: {
      [BATTLE_FRAME.ROOM_STATE]: updatePlayers,
      [BATTLE_FRAME.QUESTION]: enterAnswering,
      [BATTLE_FRAME.GAME_OVER]: enterFinished
    },
    answering: {
      [BATTLE_FRAME.ROOM_STATE]: updatePlayers,
      [BATTLE_FRAME.QUESTION]: enterAnswering,
      [BATTLE_FRAME.ANSWER_RESULT]: enterRevealed,
      [BATTLE_FRAME.GAME_OVER]: enterFinished
    },
    revealed: {
      [BATTLE_FRAME.ROOM_STATE]: updatePlayers,
      [BATTLE_FRAME.QUESTION]: enterAnswering,
      [BATTLE_FRAME.GAME_OVER]: holdFinish
    },
    finished: {
      // terminal state
    }
  }

  function transition(frame: ServerFrame): void {
    const type = String(frame.type ?? '')
    const handler = transitions[phase.value][type]
    handler?.(frame)
  }

  /**
   * Only valid in the `answering` phase. Optimistically locks the selection so
   * the UI disables the options; the actual correctness is revealed when the
   * server sends answer_result (which transitions to `revealed`).
   */
  function submit(optionId: string): void {
    if (phase.value !== 'answering') return
    if (context.selectedAnswer) return
    context.selectedAnswer = optionId
    const frame: AnswerFrame = answerFrame(context.questionIndex, optionId)
    send(frame)
  }

  /**
   * Only valid when `pendingFinish` is true (i.e. game_over was received while
   * showing the last answer result). Moves to the `finished` report.
   */
  function confirmEnd(): void {
    if (!context.pendingFinish) return
    context.pendingFinish = false
    phase.value = 'finished'
  }

  const answeredForIndex = computed(() =>
    context.answerResult && context.answerResult.index === context.questionIndex
      ? context.answerResult
      : null
  )

  return {
    phase,
    context,
    transition,
    submit,
    confirmEnd,
    answeredForIndex
  }
}
