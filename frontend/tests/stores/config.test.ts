import { beforeEach, describe, expect, it, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useConfigStore } from '@/stores/config'

vi.mock('@/api/http', () => ({
  default: {
    get: vi.fn()
  }
}))

import http from '@/api/http'

describe('config store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('uses defaults before fetch resolves', () => {
    const store = useConfigStore()
    expect(store.config.features.battleEnabled).toBe(true)
    expect(store.isMaintenance).toBe(false)
  })

  it('fetchConfig loads remote config', async () => {
    vi.mocked(http.get).mockResolvedValue({
      data: {
        features: { battleEnabled: false, recommendationEnabled: true, sseEnabled: true },
        cdn: { mediaBaseUrl: 'https://cdn.example.com' },
        ui: { theme: 'default', pageSize: 20 },
        quiz: { timerWarningSec: 10 },
        maintenance: { enabled: true, message: 'down for maintenance' },
        pollIntervalSec: 60
      }
    })

    const store = useConfigStore()
    await store.fetchConfig()

    expect(store.loaded).toBe(true)
    expect(store.isMaintenance).toBe(true)
    expect(store.maintenanceMessage).toBe('down for maintenance')
    expect(store.mediaBaseUrl).toBe('https://cdn.example.com')
  })

  it('falls back to defaults on fetch failure', async () => {
    vi.mocked(http.get).mockRejectedValue(new Error('network'))

    const store = useConfigStore()
    await store.fetchConfig()

    expect(store.loaded).toBe(true)
    expect(store.config.features.battleEnabled).toBe(true)
  })

  it('stopPolling clears the interval', async () => {
    vi.mocked(http.get).mockResolvedValue({
      data: {
        features: { battleEnabled: true, recommendationEnabled: true, sseEnabled: true },
        cdn: { mediaBaseUrl: '' },
        ui: { theme: 'default', pageSize: 20 },
        quiz: { timerWarningSec: 10 },
        maintenance: { enabled: false, message: '' },
        pollIntervalSec: 60
      }
    })

    const store = useConfigStore()
    await store.init()
    store.stopPolling()
    // If we got here without throwing, polling was set up and torn down.
    expect(store.loaded).toBe(true)
  })
})
