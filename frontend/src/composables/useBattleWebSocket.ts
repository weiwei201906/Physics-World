import { onUnmounted, ref } from 'vue'
import {
  BATTLE_FRAME,
  buildBattleSocketUrl,
  tokenUpdateFrame,
  type BattleClientFrame
} from '@/api/battle'

const BASE_RECONNECT_MS = 1000
const MAX_RECONNECT_MS = 30000

/**
 * Exponential backoff with a hard cap. Attempt starts at 1 so the first retry
 * waits one second; later retries never exceed the cap.
 */
export function computeReconnectDelay(
  attempt: number,
  baseMs: number = BASE_RECONNECT_MS,
  maxMs: number = MAX_RECONNECT_MS
): number {
  const safeAttempt = Math.max(1, Math.floor(attempt))
  const delay = baseMs * Math.pow(2, safeAttempt - 1)
  return Math.min(Math.max(0, maxMs), delay)
}

export type BattleTokenRefresh = () => Promise<{ accessToken: string } | null>

/**
 * Battle token renewal contract: the server sends token_refresh_required, the
 * client refreshes, then returns the new access token with a token_update frame.
 * The boolean result tells the socket whether the channel is still usable.
 */
export async function refreshBattleToken(
  refresh: BattleTokenRefresh,
  send: (frame: BattleClientFrame) => void
): Promise<boolean> {
  try {
    const result = await refresh()
    if (!result || !result.accessToken) return false
    send(tokenUpdateFrame(result.accessToken))
    return true
  } catch {
    return false
  }
}

export interface BattleWebSocketOptions {
  refreshToken?: BattleTokenRefresh
  onRevoked?: () => void
  onRefreshFailed?: () => void
  onFrame?: (data: unknown) => void
}

export function useBattleWebSocket(
  tokenGetter: () => string | null,
  options: BattleWebSocketOptions = {}
) {
  const connected = ref(false)
  const refreshing = ref(false)
  const messages = ref<unknown[]>([])
  const onMessageHandlers = new Set<(data: unknown) => void>()

  let socket: WebSocket | null = null
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null
  let reconnectAttempt = 0
  let closedByUser = false

  function buildUrl(): string | null {
    const token = tokenGetter()
    if (!token) return null
    return buildBattleSocketUrl(token)
  }

  function clearReconnectTimer(): void {
    if (reconnectTimer !== null) {
      clearTimeout(reconnectTimer)
      reconnectTimer = null
    }
  }

  function scheduleReconnect(): void {
    if (closedByUser) return
    clearReconnectTimer()
    reconnectAttempt += 1
    const delay = computeReconnectDelay(reconnectAttempt)
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null
      open()
    }, delay)
  }

  function dispatch(data: unknown): void {
    messages.value = [...messages.value, data]
    onMessageHandlers.forEach((handler) => handler(data))
    options.onFrame?.(data)
  }

  async function handleTokenRefreshRequired(): Promise<void> {
    if (refreshing.value) return
    refreshing.value = true
    try {
      const refreshed = await refreshBattleToken(
        options.refreshToken ?? (async () => null),
        send
      )
      if (!refreshed) {
        options.onRefreshFailed?.()
      }
    } finally {
      refreshing.value = false
    }
  }

  function handleFrame(data: unknown): void {
    const frame = data as { type?: string }
    if (frame.type === BATTLE_FRAME.TOKEN_REFRESH_REQUIRED) {
      void handleTokenRefreshRequired()
      return
    }
    if (frame.type === BATTLE_FRAME.REVOKED) {
      closedByUser = true
      disconnect()
      options.onRevoked?.()
      return
    }
    dispatch(data)
  }

  function open(): void {
    const url = buildUrl()
    if (!url) return
    if (socket) {
      socket.onclose = null
      socket.close()
      socket = null
    }

    const nextSocket = new WebSocket(url)
    socket = nextSocket

    nextSocket.onopen = () => {
      connected.value = true
      reconnectAttempt = 0
    }

    nextSocket.onmessage = (event) => {
      let data: unknown
      try {
        data = JSON.parse(event.data)
      } catch {
        return
      }
      handleFrame(data)
    }

    nextSocket.onclose = () => {
      connected.value = false
      socket = null
      scheduleReconnect()
    }

    nextSocket.onerror = () => {
      connected.value = false
    }
  }

  function connect(): void {
    closedByUser = false
    clearReconnectTimer()
    reconnectAttempt = 0
    open()
  }

  function send(frame: BattleClientFrame): void {
    if (socket && socket.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify(frame))
    }
  }

  function onMessage(handler: (data: unknown) => void): () => void {
    onMessageHandlers.add(handler)
    return () => onMessageHandlers.delete(handler)
  }

  function disconnect(): void {
    closedByUser = true
    clearReconnectTimer()
    if (socket) {
      socket.onclose = null
      socket.close()
      socket = null
    }
    connected.value = false
  }

  onUnmounted(() => {
    disconnect()
  })

  return { connected, refreshing, messages, connect, send, onMessage, disconnect }
}
