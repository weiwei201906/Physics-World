import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { login as loginApi, register as registerApi, refreshToken, logout as logoutApi, type LoginResponse } from '@/api/auth'
import { registerAuthAccessors } from '@/api/http'
import type { User } from '@/types'

const PRE_REFRESH_SEC = 120
const SILENT_RESTORE_TIMEOUT_MS = 5000

interface BroadcastMessage {
  type: 'token_refreshed' | 'logout'
  exp?: number
}

export const useAuthStore = defineStore('auth', () => {
  // Access token lives ONLY in memory, never localStorage.
  const accessToken = ref<string | null>(null)
  const tokenExp = ref<number | null>(null)
  const user = ref<User | null>(null)
  const isRestoring = ref(true)
  const preRefreshTimer = ref<ReturnType<typeof setTimeout> | null>(null)
  const broadcastChannel = ref<BroadcastChannel | null>(null)

  const isAuthenticated = computed(() => !!accessToken.value && !!user.value)
  const isAdmin = computed(() => user.value?.roles.includes('admin') ?? false)

  function clearAuthState(): void {
    accessToken.value = null
    tokenExp.value = null
    user.value = null
    if (preRefreshTimer.value) {
      clearTimeout(preRefreshTimer.value)
      preRefreshTimer.value = null
    }
  }

  function schedulePreRefresh(): void {
    if (!tokenExp.value) return
    // Pre-refresh PRE_REFRESH_SEC before expiry, based on server-given exp.
    const msUntilRefresh = Math.max(0, (tokenExp.value - PRE_REFRESH_SEC) * 1000 - Date.now())
    if (preRefreshTimer.value) clearTimeout(preRefreshTimer.value)
    preRefreshTimer.value = setTimeout(() => {
      void performRefresh().catch(() => {
        // Pre-refresh failure: degrade to login.
        degradeToLogin()
      })
    }, msUntilRefresh)
  }

  function applyLoginResponse(resp: LoginResponse): void {
    accessToken.value = resp.accessToken
    tokenExp.value = resp.exp
    user.value = {
      id: resp.user.id,
      email: resp.user.email,
      nickname: resp.user.nickname,
      avatarUrl: resp.user.avatarUrl,
      roles: resp.user.roles
    }
    schedulePreRefresh()
    broadcastTokenRefreshed(resp.exp)
  }

  async function performRefresh(): Promise<{ accessToken: string; exp: number } | null> {
    const result = await refreshToken()
    accessToken.value = result.accessToken
    tokenExp.value = result.exp
    schedulePreRefresh()
    broadcastTokenRefreshed(result.exp)
    return result
  }

  function degradeToLogin(): void {
    clearAuthState()
    if (typeof window !== 'undefined') {
      window.location.href = '/login'
    }
  }

  // --- Multi-tab coordination via BroadcastChannel ---
  function initBroadcastChannel(): void {
    if (typeof BroadcastChannel === 'undefined') return
    broadcastChannel.value = new BroadcastChannel('physlab-auth')
    broadcastChannel.value.onmessage = (event: MessageEvent<BroadcastMessage>) => {
      const msg = event.data
      if (msg.type === 'token_refreshed' && msg.exp !== undefined) {
        // Another tab refreshed; update our known expiry but do not refresh now.
        // We will refresh when our own timer fires or we get a 401.
        tokenExp.value = msg.exp
      } else if (msg.type === 'logout') {
        degradeToLogin()
      }
    }
  }

  function broadcastTokenRefreshed(exp: number): void {
    broadcastChannel.value?.postMessage({ type: 'token_refreshed', exp } satisfies BroadcastMessage)
  }

  function broadcastLogout(): void {
    broadcastChannel.value?.postMessage({ type: 'logout' } satisfies BroadcastMessage)
  }

  // --- Public actions ---

  /**
   * Page-load silent restore. Call before app mount.
   * Refreshes access token from the httpOnly refresh cookie.
   */
  async function restoreSession(): Promise<void> {
    initBroadcastChannel()
    registerAuthAccessors(
      () => accessToken.value,
      performRefresh
    )

    const controller = new AbortController()
    const timeoutId = setTimeout(() => controller.abort(), SILENT_RESTORE_TIMEOUT_MS)

    try {
      await performRefresh()
    } catch {
      // 401 or timeout: treat as not logged in.
      clearAuthState()
    } finally {
      clearTimeout(timeoutId)
      isRestoring.value = false
    }
  }

  async function login(email: string, password: string): Promise<void> {
    const resp = await loginApi({ email, password })
    applyLoginResponse(resp)
  }

  async function register(email: string, password: string, nickname: string): Promise<void> {
    const resp = await registerApi({ email, password, nickname })
    applyLoginResponse(resp)
  }

  async function logout(): Promise<void> {
    try {
      await logoutApi()
    } catch {
      // Ignore server errors; still clear local state.
    }
    broadcastLogout()
    clearAuthState()
    if (typeof window !== 'undefined') {
      window.location.href = '/login'
    }
  }

  return {
    accessToken,
    user,
    isRestoring,
    isAuthenticated,
    isAdmin,
    restoreSession,
    login,
    register,
    logout,
    degradeToLogin
  }
})
