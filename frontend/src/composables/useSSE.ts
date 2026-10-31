import { ref, onUnmounted } from 'vue'

/**
 * SSE composable for /events push channel.
 * Token passed via query param (EventSource cannot set custom headers).
 */
export function useSSE(tokenGetter: () => string | null) {
  const eventSource = ref<EventSource | null>(null)
  const connected = ref(false)
  const onMessageHandlers = new Set<(data: unknown) => void>()

  function buildUrl(): string | null {
    const token = tokenGetter()
    if (!token) return null
    const base = import.meta.env.VITE_SSE_EVENTS_URL ?? '/events'
    const separator = base.includes('?') ? '&' : '?'
    return `${base}${separator}token=${encodeURIComponent(token)}`
  }

  function connect(): void {
    const url = buildUrl()
    if (!url) return
    disconnect()

    const source = new EventSource(url)
    eventSource.value = source

    source.onopen = () => {
      connected.value = true
    }

    source.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data)
        onMessageHandlers.forEach((handler) => handler(data))
      } catch {
        // Ignore non-JSON payloads.
      }
    }

    source.onerror = () => {
      connected.value = false
    }
  }

  function onMessage(handler: (data: unknown) => void): () => void {
    onMessageHandlers.add(handler)
    return () => onMessageHandlers.delete(handler)
  }

  function disconnect(): void {
    eventSource.value?.close()
    eventSource.value = null
    connected.value = false
  }

  onUnmounted(() => {
    disconnect()
  })

  return { connected, connect, onMessage, disconnect }
}
