import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { ProfileSummary } from '@/types'
import { fetchProfileSummary } from '@/api/recommendation'

export const useProfileStore = defineStore('profile', () => {
  const summary = ref<ProfileSummary | null>(null)
  const loading = ref(false)
  const error = ref<unknown>(null)

  async function loadSummary(): Promise<void> {
    loading.value = true
    error.value = null
    try {
      summary.value = await fetchProfileSummary()
    } catch (err) {
      error.value = err
    } finally {
      loading.value = false
    }
  }

  function reset(): void {
    summary.value = null
    error.value = null
  }

  return { summary, loading, error, loadSummary, reset }
})
