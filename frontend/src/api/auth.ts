import http, { type RefreshResponse } from './http'
import type { User } from '@/types'

export interface LoginResponse {
  accessToken: string
  exp: number
  user: User
}

export interface RegisterInput {
  email: string
  password: string
  nickname: string
}

export interface LoginInput {
  email: string
  password: string
}

/**
 * POST /auth/login
 * Returns the access token in the body and rotates the refresh cookie.
 */
export async function login(input: LoginInput, signal?: AbortSignal): Promise<LoginResponse> {
  const { data } = await http.post<LoginResponse>('/auth/login', input, { signal })
  return data
}

/**
 * POST /auth/register
 * Creates the account and returns tokens immediately.
 */
export async function register(input: RegisterInput, signal?: AbortSignal): Promise<LoginResponse> {
  const { data } = await http.post<LoginResponse>('/auth/register', input, { signal })
  return data
}

/**
 * POST /auth/refresh-token
 * No body; the refresh token travels in the httpOnly cookie. 200 returns a new
 * access token plus a rotated cookie, 401 means degradation.
 */
export async function refreshToken(signal?: AbortSignal): Promise<RefreshResponse> {
  const { data } = await http.post<RefreshResponse>('/auth/refresh-token', undefined, { signal })
  return data
}

/**
 * POST /auth/logout
 * Revokes the refresh token server side.
 */
export async function logout(signal?: AbortSignal): Promise<void> {
  await http.post('/auth/logout', undefined, { signal })
}
