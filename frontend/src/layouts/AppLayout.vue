<script setup lang="ts">
import { computed } from 'vue'
import { RouterView, useRoute } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { useConfigStore } from '@/stores/config'

const auth = useAuthStore()
const config = useConfigStore()
const route = useRoute()

const navItems = [
  { name: 'home', label: 'Dashboard', path: '/' },
  { name: 'practice', label: 'Practice', path: '/practice' },
  { name: 'recommendation', label: 'Recommended', path: '/recommendation' },
  { name: 'battle', label: 'Battle', path: '/battle' },
  { name: 'leaderboard', label: 'Leaderboard', path: '/leaderboard' },
  { name: 'profile', label: 'Profile', path: '/profile' }
]

const isActive = (path: string) => {
  if (path === '/') return route.path === '/'
  return route.path.startsWith(path)
}

const initials = computed(() => {
  const name = auth.user?.nickname ?? auth.user?.email ?? '?'
  return name.charAt(0).toUpperCase()
})

function handleLogout(): void {
  void auth.logout()
}

void config.init()
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <div class="brand">
        <svg class="brand-mark" viewBox="0 0 32 32" fill="none" aria-hidden="true">
          <circle cx="16" cy="16" r="8" stroke="currentColor" stroke-width="2" />
          <ellipse cx="16" cy="16" rx="12" ry="4.5" stroke="currentColor" stroke-width="1.5" transform="rotate(30 16 16)" />
          <ellipse cx="16" cy="16" rx="12" ry="4.5" stroke="currentColor" stroke-width="1.5" transform="rotate(-30 16 16)" />
          <circle cx="16" cy="16" r="2" fill="currentColor" />
        </svg>
        <span class="brand-name">PhysLab</span>
      </div>

      <nav class="nav" aria-label="Primary">
        <RouterLink
          v-for="item in navItems"
          :key="item.name"
          :to="item.path"
          class="nav-link"
          :class="{ 'nav-link--active': isActive(item.path) }"
        >
          {{ item.label }}
        </RouterLink>
      </nav>

      <div v-if="auth.isAdmin" class="nav-section">
        <span class="nav-section-label">Admin</span>
        <RouterLink to="/admin" class="nav-link" :class="{ 'nav-link--active': isActive('/admin') }">
          Console
        </RouterLink>
      </div>

      <div class="sidebar-footer">
        <div class="user-chip">
          <div class="user-avatar">{{ initials }}</div>
          <div class="user-meta">
            <span class="user-name">{{ auth.user?.nickname }}</span>
            <span class="user-email">{{ auth.user?.email }}</span>
          </div>
        </div>
        <button class="btn btn-ghost btn-sm btn-block" @click="handleLogout">
          Sign out
        </button>
      </div>
    </aside>

    <main class="content">
      <RouterView v-slot="{ Component }">
        <transition name="fade" mode="out-in">
          <component :is="Component" />
        </transition>
      </RouterView>
    </main>
  </div>
</template>

<style scoped>
.app-shell {
  display: grid;
  grid-template-columns: var(--sidebar-width) 1fr;
  min-height: 100vh;
}

.sidebar {
  display: flex;
  flex-direction: column;
  background-color: var(--color-surface);
  border-right: 1px solid var(--color-border);
  padding: var(--space-5) var(--space-4);
  position: sticky;
  top: 0;
  height: 100vh;
}

.brand {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-bottom: var(--space-6);
  padding: 0 var(--space-1);
}

.brand-mark {
  width: 28px;
  height: 28px;
  color: var(--color-primary-700);
}

.brand-name {
  font-family: var(--font-display);
  font-weight: 700;
  font-size: var(--text-lg);
  color: var(--color-text);
}

.nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.nav-link {
  display: block;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-md);
  color: var(--color-text-muted);
  font-size: var(--text-base);
  font-weight: 500;
  text-decoration: none;
  transition: background-color var(--duration-fast) var(--ease-out),
              color var(--duration-fast) var(--ease-out);
}

.nav-link:hover {
  background-color: var(--color-bg-muted);
  color: var(--color-text);
  text-decoration: none;
}

.nav-link--active {
  background-color: var(--color-primary-50);
  color: var(--color-primary-800);
}

.nav-section {
  margin-top: var(--space-6);
  padding-top: var(--space-4);
  border-top: 1px solid var(--color-border);
}

.nav-section-label {
  display: block;
  font-size: var(--text-xs);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--color-text-faint);
  padding: 0 var(--space-3) var(--space-2);
}

.sidebar-footer {
  margin-top: auto;
  padding-top: var(--space-4);
  border-top: 1px solid var(--color-border);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.user-chip {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2);
}

.user-avatar {
  width: 36px;
  height: 36px;
  border-radius: 50%;
  background-color: var(--color-primary-700);
  color: var(--color-text-on-primary);
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 600;
  font-size: var(--text-sm);
  flex-shrink: 0;
}

.user-meta {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.user-name {
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--color-text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.user-email {
  font-size: var(--text-xs);
  color: var(--color-text-faint);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.content {
  min-width: 0;
  padding: var(--space-6);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity var(--duration-base) var(--ease-out);
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

@media (max-width: 768px) {
  .app-shell {
    grid-template-columns: 1fr;
  }
  .sidebar {
    position: static;
    height: auto;
    flex-direction: row;
    overflow-x: auto;
  }
  .sidebar-footer,
  .nav-section {
    display: none;
  }
}
</style>
