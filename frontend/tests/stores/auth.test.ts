import { beforeEach, describe, expect, it, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useAuthStore } from '@/stores/auth'

vi.mock('@/api/auth', () => ({
  login: vi.fn(),
  register: vi.fn(),
  refreshToken: vi.fn(),
  logout: vi.fn()
}))

vi.mock('@/api/http', () => ({
  default: {},
  registerAuthAccessors: vi.fn()
}))

import { login, register, refreshToken, logout } from '@/api/auth'

describe('auth store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    vi.stubGlobal('BroadcastChannel', class {
      onmessage: ((ev: MessageEvent) => void) | null = null
      postMessage() {}
      close() {}
    })
    // jsdom does not implement navigation; stub the location setter.
    Object.defineProperty(window, 'location', {
      value: { href: '', assign: vi.fn() },
      writable: true,
      configurable: true
    })
  })

  it('starts unauthenticated', () => {
    const store = useAuthStore()
    expect(store.isAuthenticated).toBe(false)
    expect(store.accessToken).toBeNull()
    expect(store.user).toBeNull()
  })

  it('login stores token and user in memory only', async () => {
    vi.mocked(login).mockResolvedValue({
      accessToken: 'token-123',
      exp: Math.floor(Date.now() / 1000) + 900,
      user: { id: 'u1', email: 'a@b.com', nickname: 'Alice', avatarUrl: null, roles: [] }
    })

    const store = useAuthStore()
    await store.login('a@b.com', 'pw')

    expect(store.accessToken).toBe('token-123')
    expect(store.isAuthenticated).toBe(true)
    expect(store.user?.nickname).toBe('Alice')
  })

  it('logout clears all auth state', async () => {
    vi.mocked(logout).mockResolvedValue()
    vi.mocked(login).mockResolvedValue({
      accessToken: 'token-123',
      exp: Math.floor(Date.now() / 1000) + 900,
      user: { id: 'u1', email: 'a@b.com', nickname: 'Alice', avatarUrl: null, roles: [] }
    })

    const store = useAuthStore()
    await store.login('a@b.com', 'pw')
    expect(store.isAuthenticated).toBe(true)

    await store.logout()
    expect(store.isAuthenticated).toBe(false)
    expect(store.accessToken).toBeNull()
    expect(store.user).toBeNull()
  })

  it('silent restore succeeds when refresh token is valid', async () => {
    vi.mocked(refreshToken).mockResolvedValue({
      accessToken: 'restored-token',
      exp: Math.floor(Date.now() / 1000) + 900
    })

    const store = useAuthStore()
    await store.restoreSession()

    expect(store.accessToken).toBe('restored-token')
    expect(store.isRestoring).toBe(false)
  })

  it('silent restore fails gracefully on 401', async () => {
    vi.mocked(refreshToken).mockRejectedValue(new Error('401'))

    const store = useAuthStore()
    await store.restoreSession()

    expect(store.accessToken).toBeNull()
    expect(store.isAuthenticated).toBe(false)
    expect(store.isRestoring).toBe(false)
  })

  it('register stores token and user', async () => {
    vi.mocked(register).mockResolvedValue({
      accessToken: 'new-token',
      exp: Math.floor(Date.now() / 1000) + 900,
      user: { id: 'u2', email: 'c@d.com', nickname: 'Bob', avatarUrl: null, roles: [] }
    })

    const store = useAuthStore()
    await store.register('c@d.com', 'pw', 'Bob')

    expect(store.accessToken).toBe('new-token')
    expect(store.user?.nickname).toBe('Bob')
  })

  it('exposes admin role via computed', async () => {
    vi.mocked(login).mockResolvedValue({
      accessToken: 't',
      exp: Math.floor(Date.now() / 1000) + 900,
      user: { id: 'u1', email: 'a@b.com', nickname: 'A', avatarUrl: null, roles: ['admin'] }
    })

    const store = useAuthStore()
    await store.login('a@b.com', 'pw')
    expect(store.isAdmin).toBe(true)
  })
})
