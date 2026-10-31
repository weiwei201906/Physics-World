export interface GuardAuthState {
  isAuthenticated: boolean
  isAdmin: boolean
}

export interface GuardTarget {
  fullPath: string
  meta: {
    public?: boolean
    admin?: boolean
  }
}

export type NavigationDecision =
  | true
  | { name: 'login'; query: { redirect: string } }
  | { name: 'home' }

/**
 * Pure route guard used by the router. Unauthenticated users are sent to login
 * with a redirect target; authenticated non-admins never reach the console.
 */
export function resolveNavigation(
  to: GuardTarget,
  auth: GuardAuthState
): NavigationDecision {
  if (to.meta.public) return true
  if (!auth.isAuthenticated) {
    return { name: 'login', query: { redirect: to.fullPath } }
  }
  if (to.meta.admin && !auth.isAdmin) {
    return { name: 'home' }
  }
  return true
}
