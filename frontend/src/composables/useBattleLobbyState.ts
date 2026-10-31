import { ref } from 'vue'
import { BATTLE_FRAME, type BattleClientFrame } from '@/api/battle'
import type { BattleMode } from '@/types'

/**
 * Matchmaking lifecycle for the battle lobby.
 *
 *   idle ──findMatch──▶ matching ──match_found──▶ matched (navigate to room)
 *                         │
 *                         └──error──▶ error ──findMatch──▶ matching
 *                         └──cancel──▶ idle
 */
export type LobbyPhase = 'idle' | 'matching' | 'matched' | 'error'

export interface LobbyContext {
  mode: BattleMode
  errorMessage: string
  roomId: string | null
}

type ServerFrame = Record<string, unknown>

export function useBattleLobbyState(
  send: (frame: BattleClientFrame) => void,
  onMatched: (roomId: string) => void,
  fallbackError: string
) {
  const phase = ref<LobbyPhase>('idle')

  const context = {
    mode: '1v1' as BattleMode,
    errorMessage: '',
    roomId: null as string | null
  }

  function selectMode(mode: BattleMode): void {
    if (phase.value === 'matching') return
    context.mode = mode
  }

  function findMatch(frame: BattleClientFrame): void {
    if (phase.value === 'matching') return
    context.errorMessage = ''
    context.roomId = null
    phase.value = 'matching'
    send(frame)
  }

  function cancel(frame: BattleClientFrame): void {
    if (phase.value !== 'matching') return
    phase.value = 'idle'
    send(frame)
  }

  function transition(frame: ServerFrame): void {
    const type = String(frame.type ?? '')
    if (phase.value !== 'matching') return
    if (type === BATTLE_FRAME.MATCH_FOUND) {
      const roomId = String(frame.roomId ?? '')
      if (!roomId) return
      context.roomId = roomId
      phase.value = 'matched'
      onMatched(roomId)
      return
    }
    if (type === BATTLE_FRAME.ERROR) {
      context.errorMessage = String(frame.message ?? '') || fallbackError
      phase.value = 'error'
    }
  }

  return { phase, context, transition, selectMode, findMatch, cancel }
}
