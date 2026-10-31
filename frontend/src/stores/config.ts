import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import http from '@/api/http'
import type { WebConfig } from '@/types'

const DEFAULT_CONFIG: WebConfig = {
  features: {
    battleEnabled: true,
    recommendationEnabled: true,
    sseEnabled: true
  },
  cdn: { mediaBaseUrl: '' },
  ui: { theme: 'default', pageSize: 20 },
  quiz: { timerWarningSec: 10 },
  maintenance: { enabled: false, message: '' },
  pollIntervalSec: 60
}

export const useConfigStore = defineStore('config', () => {
  const config = ref<WebConfig>(DEFAULT_CONFIG)
  const loaded = ref(false)
  let pollTimer: ReturnType<typeof setInterval> | null = null

  const isMaintenance = computed(() => config.value.maintenance.enabled)
  const maintenanceMessage = computed(() => config.value.maintenance.message)
  const mediaBaseUrl = computed(() => config.value.cdn.mediaBaseUrl)

  async function fetchConfig(): Promise<void> {
    try {
      const { data } = await http.get<WebConfig>('/config', { params: { ns: 'web' } })
      config.value = { ...DEFAULT_CONFIG, ...data }
    } catch {
      // Fall back to defaults; non-fatal.
      config.value = DEFAULT_CONFIG
    } finally {
      loaded.value = true
    }
  }

  function startPolling(): void {
    if (pollTimer) clearInterval(pollTimer)
    pollTimer = setInterval(() => {
      void fetchConfig()
    }, config.value.pollIntervalSec * 1000)
  }

  function stopPolling(): void {
    if (pollTimer) {
      clearInterval(pollTimer)
      pollTimer = null
    }
  }

  async function init(): Promise<void> {
    await fetchConfig()
    startPolling()
  }

  return {
    config,
    loaded,
    isMaintenance,
    maintenanceMessage,
    mediaBaseUrl,
    fetchConfig,
    init,
    stopPolling
  }
})
