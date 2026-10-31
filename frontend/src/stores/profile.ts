import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { ProfileSummary } from '@/types'
import { fetchProfileSummary } from '@/api/recommendation'

export const useProfileStore = defineStore('profile', () => {
  const summary = ref<ProfileSummary | null>(null)
  const loading = ref(false)

  async function loadSummary(): Promise<void> {
    loading.value = true
    try {
      summary.value = await fetchProfileSummary()
    } finally {
      loading.value = false
    }
  }

  return { summary, loading, loadSummary }
})
