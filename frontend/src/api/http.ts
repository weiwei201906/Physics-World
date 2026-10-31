import axios, { type AxiosInstance, type AxiosError, type InternalAxiosRequestConfig } from 'axios'

const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? ''

export interface RefreshResponse {
  accessToken: string
  exp: number
}

type QueuedRequest = {
  resolve: (token: string) => void
  reject: (error: unknown) => void
}

/**
 * Single-flight token refresh state.
 * Only one refresh request may be in flight at a time; concurrent 401s wait.
 */
let isRefreshing = false
let refreshQueue: QueuedRequest[] = []

/**
 * Resolve the access token from the auth store (set by the app bootstrap).
 * We avoid importing the store directly to prevent circular dependency at
 * module load time; instead the store registers a getter.
 */
let accessTokenGetter: () => string | null = () => null
let refreshTrigger: () => Promise<RefreshResponse | null> = async () => null

export function registerAuthAccessors(
  getter: () => string | null,
  refresher: () => Promise<RefreshResponse | null>
): void {
  accessTokenGetter = getter
  refreshTrigger = refresher
}

function onRefreshSuccess(token: string): void {
  refreshQueue.forEach(({ resolve }) => resolve(token))
  refreshQueue = []
}

function onRefreshFailure(error: unknown): void {
  refreshQueue.forEach(({ reject }) => reject(error))
  refreshQueue = []
}

/**
 * Enqueue a request waiting for the in-flight refresh.
 * Returns a promise that resolves with the new token.
 */
function waitForRefresh(): Promise<string> {
  return new Promise((resolve, reject) => {
    refreshQueue.push({ resolve, reject })
  })
}

const http: AxiosInstance = axios.create({
  baseURL: API_BASE_URL,
  timeout: 15000,
  withCredentials: true
})

// Request interceptor: attach access token from memory store.
http.interceptors.request.use((config) => {
  const token = accessTokenGetter()
  if (token && config.headers) {
    config.headers.Authorization = `Bearer ${token}`
  }
  return config
})

// Response interceptor: 401 triggers single-flight refresh.
http.interceptors.response.use(
  (response) => response,
  async (error: AxiosError) => {
    const originalRequest = error.config as InternalAxiosRequestConfig & { _retried?: boolean }

    // Only refresh on 401, and only once per request.
    if (error.response?.status !== 401 || originalRequest._retried) {
      return Promise.reject(error)
    }

    originalRequest._retried = true

    if (isRefreshing) {
      // Another refresh is in flight; wait for it.
      try {
        const newToken = await waitForRefresh()
        if (originalRequest.headers) {
          originalRequest.headers.Authorization = `Bearer ${newToken}`
        }
        return http(originalRequest)
      } catch {
        return Promise.reject(error)
      }
    }

    isRefreshing = true
    try {
      const result = await refreshTrigger()
      if (!result) {
        onRefreshFailure(error)
        return Promise.reject(error)
      }
      onRefreshSuccess(result.accessToken)
      if (originalRequest.headers) {
        originalRequest.headers.Authorization = `Bearer ${result.accessToken}`
      }
      return http(originalRequest)
    } catch (refreshError) {
      onRefreshFailure(refreshError)
      return Promise.reject(refreshError)
    } finally {
      isRefreshing = false
    }
  }
)

export default http
