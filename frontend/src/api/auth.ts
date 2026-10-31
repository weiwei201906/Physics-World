import http, { type RefreshResponse } from './http'

export interface LoginResponse {
  accessToken: string
  exp: number
  user: {
    id: string
    email: string
    nickname: string
    avatarUrl: string | null
    roles: string[]
  }
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
 * Returns access token (body) + sets refresh token via httpOnly cookie.
 */
export async function login(input: LoginInput): Promise<LoginResponse> {
  const { data } = await http.post<LoginResponse>('/auth/login', input)
  return data
}

/**
 * POST /auth/register
 * Creates user and returns tokens immediately.
 */
export async function register(input: RegisterInput): Promise<LoginResponse> {
  const { data } = await http.post<LoginResponse>('/auth/register', input)
  return data
}

/**
 * POST /auth/refresh-token
 * No body; refresh token comes from httpOnly cookie.
 * Returns 200 with new access token + rotated cookie, or 401 on failure.
 */
export async function refreshToken(): Promise<RefreshResponse> {
  const { data } = await http.post<RefreshResponse>('/auth/refresh-token')
  return data
}

/**
 * POST /auth/logout
 * Revokes refresh token on the server.
 */
export async function logout(): Promise<void> {
  await http.post('/auth/logout')
}
