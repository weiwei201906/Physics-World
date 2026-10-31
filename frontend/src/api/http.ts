import axios, { type AxiosInstance, type AxiosError, type InternalAxiosRequestConfig } from 'axios'

const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? ''
const DEFAULT_TIMEOUT_MS = 15000

export interface RefreshResponse {
  accessToken: string
  exp: number
}

type QueuedRequest = {
  resolve: (token: string) => void
  reject: (error: unknown) => void
}

/**
 * Single-flight refresh state shared by every request issued through this
 * Axios instance. Concurrent 401s wait for the one in-flight refresh.
 */
let isRefreshing = false
let refreshQueue: QueuedRequest[] = []

let accessTokenGetter: () => string | null = () => null
let refreshTrigger: () => Promise<RefreshResponse | null> = async () => null
let authFailureHandler: () => void = () => {}

export function registerAuthAccessors(
  getter: () => string | null,
  refresher: () => Promise<RefreshResponse | null>
): void {
  accessTokenGetter = getter
  refreshTrigger = refresher
}

/**
 * Called when the refresh chain is exhausted: refresh failed, or a replayed
 * request still received 401. The auth store uses this for atomic degradation.
 */
export function registerAuthFailureHandler(handler: () => void): void {
  authFailureHandler = handler
}

function notifyAuthFailure(): void {
  try {
    authFailureHandler()
  } catch {
    // Degradation must never break the interceptor chain.
  }
}

function onRefreshSuccess(token: string): void {
  const queue = refreshQueue
  refreshQueue = []
  queue.forEach(({ resolve }) => resolve(token))
}

function onRefreshFailure(error: unknown): void {
  const queue = refreshQueue
  refreshQueue = []
  queue.forEach(({ reject }) => reject(error))
}

function waitForRefresh(): Promise<string> {
  return new Promise((resolve, reject) => {
    refreshQueue.push({ resolve, reject })
  })
}

function retryWithToken(
  instance: AxiosInstance,
  request: InternalAxiosRequestConfig,
  token: string
): Promise<unknown> {
  if (request.headers) {
    request.headers.Authorization = `Bearer ${token}`
  }
  return instance(request)
}

const http: AxiosInstance = axios.create({
  baseURL: API_BASE_URL,
  timeout: DEFAULT_TIMEOUT_MS,
  withCredentials: true
})

http.interceptors.request.use((config) => {
  const token = accessTokenGetter()
  if (token && config.headers) {
    config.headers.Authorization = `Bearer ${token}`
  }
  return config
})

http.interceptors.response.use(
  (response) => response,
  async (error: AxiosError) => {
    const originalRequest = error.config as (InternalAxiosRequestConfig & { _retried?: boolean }) | undefined
    const status = error.response?.status

    if (!originalRequest || status !== 401) {
      return Promise.reject(error)
    }

    const url = originalRequest.url ?? ''
    // Auth endpoints report their own failures; never loop a refresh here.
    if (url.includes('/auth/')) {
      return Promise.reject(error)
    }

    if (originalRequest._retried) {
      // New token was immediately rejected (for example an account ban).
      notifyAuthFailure()
      return Promise.reject(error)
    }

    originalRequest._retried = true

    if (isRefreshing) {
      try {
        const token = await waitForRefresh()
        return await retryWithToken(http, originalRequest, token)
      } catch {
        return Promise.reject(error)
      }
    }

    isRefreshing = true
    try {
      const result = await refreshTrigger()
      if (!result || !result.accessToken) {
        onRefreshFailure(error)
        notifyAuthFailure()
        return Promise.reject(error)
      }
      onRefreshSuccess(result.accessToken)
      return await retryWithToken(http, originalRequest, result.accessToken)
    } catch (refreshError) {
      onRefreshFailure(refreshError)
      notifyAuthFailure()
      return Promise.reject(refreshError)
    } finally {
      isRefreshing = false
    }
  }
)

export default http
