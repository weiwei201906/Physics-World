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
  quiz: { timerWarningSec: 10, examDurationSec: 3600 },
  maintenance: { enabled: false, message: '' },
  pollIntervalSec: 60,
  auth: { preRefreshSec: 120, silentRestoreTimeoutSec: 5 }
}

interface RawWebConfig {
  values?: Partial<WebConfig>
  features?: Partial<WebConfig['features']>
  cdn?: Partial<WebConfig['cdn']>
  ui?: Partial<WebConfig['ui']>
  quiz?: Partial<WebConfig['quiz']>
  maintenance?: Partial<WebConfig['maintenance']>
  pollIntervalSec?: number
  auth?: Partial<WebConfig['auth']>
}

function mergeConfig(raw: RawWebConfig | null | undefined): WebConfig {
  if (!raw) return { ...DEFAULT_CONFIG }
  const source = raw.values ?? raw
  return {
    features: { ...DEFAULT_CONFIG.features, ...(source.features ?? {}) },
    cdn: { ...DEFAULT_CONFIG.cdn, ...(source.cdn ?? {}) },
    ui: { ...DEFAULT_CONFIG.ui, ...(source.ui ?? {}) },
    quiz: { ...DEFAULT_CONFIG.quiz, ...(source.quiz ?? {}) },
    maintenance: { ...DEFAULT_CONFIG.maintenance, ...(source.maintenance ?? {}) },
    pollIntervalSec: source.pollIntervalSec ?? DEFAULT_CONFIG.pollIntervalSec,
    auth: { ...DEFAULT_CONFIG.auth, ...(source.auth ?? {}) }
  }
}

export const useConfigStore = defineStore('config', () => {
  const config = ref<WebConfig>(mergeConfig(null))
  const loaded = ref(false)
  const loadFailed = ref(false)
  let pollTimer: ReturnType<typeof setInterval> | null = null

  const isMaintenance = computed(() => config.value.maintenance.enabled)
  const maintenanceMessage = computed(() => config.value.maintenance.message)
  const mediaBaseUrl = computed(() => config.value.cdn.mediaBaseUrl)
  const pageSize = computed(() => config.value.ui.pageSize)
  const timerWarningSec = computed(() => config.value.quiz.timerWarningSec)
  const examDurationSec = computed(() => config.value.quiz.examDurationSec)
  const auth = computed(() => config.value.auth)

  async function fetchConfig(): Promise<void> {
    try {
      const { data } = await http.get<RawWebConfig>('/config', { params: { ns: 'web' } })
      config.value = mergeConfig(data)
      loadFailed.value = false
    } catch {
      config.value = mergeConfig(null)
      loadFailed.value = true
    } finally {
      loaded.value = true
    }
  }

  function startPolling(): void {
    stopPolling()
    const intervalSec = config.value.pollIntervalSec > 0 ? config.value.pollIntervalSec : DEFAULT_CONFIG.pollIntervalSec
    pollTimer = setInterval(() => {
      void fetchConfig()
    }, intervalSec * 1000)
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
    loadFailed,
    isMaintenance,
    maintenanceMessage,
    mediaBaseUrl,
    pageSize,
    timerWarningSec,
    examDurationSec,
    auth,
    fetchConfig,
    init,
    startPolling,
    stopPolling
  }
})
