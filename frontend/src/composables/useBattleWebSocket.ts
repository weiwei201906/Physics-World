import { ref, onUnmounted } from 'vue'

/**
 * Battle WebSocket composable.
 * Connects to /ws/battle?token=... and handles:
 *  - token refresh required messages
 *  - reconnection
 *  - message dispatch
 */
export function useBattleWebSocket(tokenGetter: () => string | null) {
  const ws = ref<WebSocket | null>(null)
  const connected = ref(false)
  const messages = ref<unknown[]>([])
  const onMessageHandlers = new Set<(data: unknown) => void>()

  function buildUrl(): string | null {
    const token = tokenGetter()
    if (!token) return null
    const base = import.meta.env.VITE_BATTLE_WS_URL ?? '/ws/battle'
    const separator = base.includes('?') ? '&' : '?'
    return `${base}${separator}token=${encodeURIComponent(token)}`
  }

  function connect(): void {
    const url = buildUrl()
    if (!url) return
    if (ws.value) ws.value.close()

    const socket = new WebSocket(url)
    ws.value = socket

    socket.onopen = () => {
      connected.value = true
    }

    socket.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data)
        messages.value.push(data)
        onMessageHandlers.forEach((handler) => handler(data))
      } catch {
        // Ignore malformed frames.
      }
    }

    socket.onclose = () => {
      connected.value = false
    }

    socket.onerror = () => {
      connected.value = false
    }
  }

  function send(data: unknown): void {
    if (ws.value && ws.value.readyState === WebSocket.OPEN) {
      ws.value.send(JSON.stringify(data))
    }
  }

  function onMessage(handler: (data: unknown) => void): () => void {
    onMessageHandlers.add(handler)
    return () => onMessageHandlers.delete(handler)
  }

  function disconnect(): void {
    ws.value?.close()
    ws.value = null
    connected.value = false
  }

  onUnmounted(() => {
    disconnect()
  })

  return { connected, messages, connect, send, onMessage, disconnect }
}
