import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import {
  login as loginApi,
  register as registerApi,
  refreshToken as refreshTokenApi,
  logout as logoutApi,
  type LoginResponse
} from '@/api/auth'
import { fetchMe } from '@/api/user'
import {
  registerAuthAccessors,
  registerAuthFailureHandler,
  type RefreshResponse
} from '@/api/http'
import { useConfigStore } from '@/stores/config'
import type { User } from '@/types'

const FALLBACK_PRE_REFRESH_SEC = 120
const FALLBACK_SILENT_TIMEOUT_SEC = 5
const MAX_TIMER_MS = 2_147_000_000
const AUTH_CHANNEL = 'particle-auth'

interface BroadcastMessage {
  type: 'token_refreshed' | 'logout'
  exp?: number
}

type SessionClearListener = () => void
const sessionClearListeners = new Set<SessionClearListener>()

export function registerSessionClearListener(listener: SessionClearListener): () => void {
  sessionClearListeners.add(listener)
  return () => sessionClearListeners.delete(listener)
}

function runSessionClearListeners(): void {
  sessionClearListeners.forEach((listener) => {
    try {
      listener()
    } catch {
      // Teardown must not prevent the remaining cleanup.
    }
  })
}

let navigateToLogin: () => void = () => {
  if (typeof window !== 'undefined') {
    window.location.href = '/login'
  }
}

export function setAuthNavigator(navigator: () => void): void {
  navigateToLogin = navigator
}

/**
 * Delay until the configured pre-refresh moment, clamped to the largest delay
 * setTimeout accepts. The calculation uses the absolute server exp, so local
 * clock drift is compensated by the server value instead of an interval.
 */
export function computePreRefreshDelay(
  expSec: number,
  preRefreshSec: number,
  nowMs: number
): number {
  const targetMs = expSec * 1000 - preRefreshSec * 1000
  const delay = targetMs - nowMs
  if (!Number.isFinite(delay)) return 0
  return Math.max(0, Math.min(delay, MAX_TIMER_MS))
}

export const useAuthStore = defineStore('auth', () => {
  // Access token lives only in Pinia memory; never localStorage or cookies.
  const accessToken = ref<string | null>(null)
  const tokenExp = ref<number | null>(null)
  const observedExp = ref<number | null>(null)
  const user = ref<User | null>(null)
  const isRestoring = ref(true)
  const preRefreshTimer = ref<ReturnType<typeof setTimeout> | null>(null)
  const broadcastChannel = ref<BroadcastChannel | null>(null)

  let refreshPromise: Promise<RefreshResponse> | null = null
  let listenersRegistered = false

  const isAuthenticated = computed(() => !!accessToken.value && !!user.value)
  const isAdmin = computed(() => user.value?.roles.includes('admin') ?? false)

  function authTiming(): { preRefreshSec: number; timeoutSec: number } {
    try {
      const config = useConfigStore()
      return {
        preRefreshSec: config.auth.preRefreshSec || FALLBACK_PRE_REFRESH_SEC,
        timeoutSec: config.auth.silentRestoreTimeoutSec || FALLBACK_SILENT_TIMEOUT_SEC
      }
    } catch {
      return {
        preRefreshSec: FALLBACK_PRE_REFRESH_SEC,
        timeoutSec: FALLBACK_SILENT_TIMEOUT_SEC
      }
    }
  }

  function clearPreRefreshTimer(): void {
    if (preRefreshTimer.value !== null) {
      clearTimeout(preRefreshTimer.value)
      preRefreshTimer.value = null
    }
  }

  function clearAuthState(): void {
    accessToken.value = null
    tokenExp.value = null
    user.value = null
    clearPreRefreshTimer()
    isRestoring.value = false
  }

  function applyUser(nextUser: User): void {
    user.value = {
      id: nextUser.id,
      email: nextUser.email,
      nickname: nextUser.nickname,
      avatarUrl: nextUser.avatarUrl,
      roles: nextUser.roles,
      createdAt: nextUser.createdAt
    }
  }

  function schedulePreRefresh(): void {
    clearPreRefreshTimer()
    if (tokenExp.value === null) return
    const { preRefreshSec } = authTiming()
    const delay = computePreRefreshDelay(tokenExp.value, preRefreshSec, Date.now())
    preRefreshTimer.value = setTimeout(() => {
      preRefreshTimer.value = null
      void forceRefresh().catch(() => {
        degradeToLogin()
      })
    }, delay)
  }

  function applyToken(result: RefreshResponse): void {
    accessToken.value = result.accessToken
    tokenExp.value = result.exp
    schedulePreRefresh()
    broadcastTokenRefreshed(result.exp)
  }

  function ensureBroadcastChannel(): void {
    if (typeof BroadcastChannel === 'undefined' || broadcastChannel.value) return
    broadcastChannel.value = new BroadcastChannel(AUTH_CHANNEL)
    broadcastChannel.value.onmessage = (event: MessageEvent<BroadcastMessage>) => {
      const msg = event.data
      if (msg.type === 'token_refreshed' && msg.exp !== undefined) {
        // Another tab rotated the shared cookie. Record the expiry but keep our
        // own timer; a 401 from our old token will trigger a local refresh.
        observedExp.value = msg.exp
      } else if (msg.type === 'logout') {
        clearAuthState()
        runSessionClearListeners()
        navigateToLogin()
      }
    }
  }

  function broadcastTokenRefreshed(exp: number): void {
    broadcastChannel.value?.postMessage({ type: 'token_refreshed', exp } satisfies BroadcastMessage)
  }

  function broadcastLogout(): void {
    broadcastChannel.value?.postMessage({ type: 'logout' } satisfies BroadcastMessage)
  }

  function registerHttpBridge(): void {
    if (listenersRegistered) return
    listenersRegistered = true
    registerAuthAccessors(
      () => accessToken.value,
      performRefresh
    )
    registerAuthFailureHandler(() => {
      degradeToLogin()
    })
  }

  /**
   * Single-flight refresh. Every caller (pre-refresh timer, 401 retry chain,
   * WS/SSE reconnects) shares one in-flight request; refresh tokens rotate once.
   */
  async function performRefresh(): Promise<RefreshResponse> {
    if (refreshPromise) return refreshPromise
    const { timeoutSec } = authTiming()
    refreshPromise = (async () => {
      const controller = new AbortController()
      const timeoutId = setTimeout(() => controller.abort(), timeoutSec * 1000)
      try {
        const result = await refreshTokenApi(controller.signal)
        applyToken(result)
        return result
      } finally {
        clearTimeout(timeoutId)
        refreshPromise = null
      }
    })()
    return refreshPromise
  }

  async function forceRefresh(): Promise<RefreshResponse> {
    return performRefresh()
  }

  function degradeToLogin(): void {
    clearAuthState()
    runSessionClearListeners()
    broadcastLogout()
    navigateToLogin()
  }

  /**
   * Boot-time silent restore: refresh from the httpOnly cookie, then load the
   * user profile. Any failure leaves the app unauthenticated without redirect.
   */
  async function restoreSession(): Promise<void> {
    ensureBroadcastChannel()
    registerHttpBridge()
    isRestoring.value = true
    const { timeoutSec } = authTiming()
    const controller = new AbortController()
    const timeoutId = setTimeout(() => controller.abort(), timeoutSec * 1000)
    try {
      await performRefresh()
      const me = await fetchMe(controller.signal)
      applyUser(me)
    } catch {
      clearAuthState()
    } finally {
      clearTimeout(timeoutId)
      isRestoring.value = false
    }
  }

  async function login(email: string, password: string): Promise<void> {
    ensureBroadcastChannel()
    registerHttpBridge()
    const resp = await loginApi({ email, password })
    applyLoginResponse(resp)
  }

  async function register(email: string, password: string, nickname: string): Promise<void> {
    ensureBroadcastChannel()
    registerHttpBridge()
    const resp = await registerApi({ email, password, nickname })
    applyLoginResponse(resp)
  }

  function applyLoginResponse(resp: LoginResponse): void {
    applyUser(resp.user)
    accessToken.value = resp.accessToken
    tokenExp.value = resp.exp
    schedulePreRefresh()
    broadcastTokenRefreshed(resp.exp)
  }

  async function logout(): Promise<void> {
    try {
      await logoutApi()
    } catch {
      // Even if the server call fails, local state is cleared atomically.
    }
    runSessionClearListeners()
    broadcastLogout()
    clearAuthState()
    navigateToLogin()
  }

  function applyProfile(nextUser: User): void {
    applyUser(nextUser)
  }

  /** Called when the server reports that the account was revoked. */
  function handleRevoked(): void {
    degradeToLogin()
  }

  return {
    accessToken,
    tokenExp,
    observedExp,
    user,
    isRestoring,
    isAuthenticated,
    isAdmin,
    restoreSession,
    login,
    register,
    logout,
    forceRefresh,
    applyProfile,
    handleRevoked,
    degradeToLogin
  }
})
