<script setup lang="ts">
import { computed } from 'vue'
import { RouterView, RouterLink, useRoute } from 'vue-router'
import { t } from '@/i18n'
import BrandMark from '@/components/BrandMark.vue'
import LanguageToggle from '@/components/LanguageToggle.vue'
import { useAuthStore } from '@/stores/auth'
import { useConfigStore } from '@/stores/config'

const auth = useAuthStore()
const config = useConfigStore()
const route = useRoute()

const navItems = [
  { name: 'home', key: 'nav.home', path: '/' },
  { name: 'practice', key: 'nav.practice', path: '/practice' },
  { name: 'chapters', key: 'nav.chapters', path: '/chapters' },
  { name: 'recommendation', key: 'nav.recommendation', path: '/recommendation' },
  { name: 'battle', key: 'nav.battle', path: '/battle' },
  { name: 'leaderboard', key: 'nav.leaderboard', path: '/leaderboard' },
  { name: 'profile', key: 'nav.profile', path: '/profile' }
] as const

function isActive(path: string): boolean {
  if (path === '/') return route.path === '/'
  return route.path.startsWith(path)
}

const initials = computed(() => {
  const name = auth.user?.nickname || auth.user?.email || '?'
  return name.charAt(0).toUpperCase()
})

function handleLogout(): void {
  void auth.logout()
}
</script>

<template>
  <div class="shell">
    <header class="masthead">
      <div class="container masthead-inner">
        <RouterLink to="/" class="brand" :aria-label="t('brand.name')">
          <BrandMark :size="30" class="brand-glyph" />
          <span class="brand-text">
            <span class="brand-word">{{ t('brand.name') }}</span>
            <span class="brand-zh">{{ t('brand.nameZh') }}</span>
          </span>
          <span class="brand-tagline">{{ t('brand.tagline') }}</span>
        </RouterLink>

        <nav class="nav" :aria-label="t('nav.menu')">
          <RouterLink
            v-for="item in navItems"
            :key="item.name"
            :to="item.path"
            class="nav-link"
            :class="{ 'nav-link--active': isActive(item.path) }"
          >
            {{ t(item.key) }}
          </RouterLink>
          <RouterLink
            v-if="auth.isAdmin"
            to="/admin"
            class="nav-link"
            :class="{ 'nav-link--active': isActive('/admin') }"
          >
            {{ t('nav.admin') }}
          </RouterLink>
        </nav>

        <div class="account">
          <LanguageToggle />
          <span class="account-avatar" :title="auth.user?.nickname ?? ''">
            <img
              v-if="auth.user?.avatarUrl"
              :src="auth.user.avatarUrl"
              :alt="auth.user.nickname"
              class="account-image"
            />
            <span v-else>{{ initials }}</span>
          </span>
          <div class="account-meta">
            <span class="account-name">{{ auth.user?.nickname }}</span>
            <span v-if="auth.isAdmin" class="account-role">{{ t('common.roleAdmin') }}</span>
          </div>
          <button type="button" class="account-logout" @click="handleLogout">
            {{ t('common.logout') }}
          </button>
        </div>
      </div>
    </header>

    <div v-if="config.isMaintenance" class="container">
      <p class="maintenance" role="status">
        {{ t('home.maintenance', { message: config.maintenanceMessage }) }}
      </p>
    </div>

    <main class="main">
      <div class="container">
        <RouterView />
      </div>
    </main>

    <footer class="colophon">
      <div class="container colophon-inner">
        <span>{{ t('brand.name') }}</span>
        <span>{{ t('brand.subtitle') }}</span>
      </div>
    </footer>
  </div>
</template>

<style scoped>
.shell {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
  background-color: var(--color-bg);
}

.masthead {
  position: sticky;
  top: 0;
  z-index: 20;
  background-color: var(--color-surface);
  border-top: 2px solid var(--color-text);
  border-bottom: 1px solid var(--color-rule);
}

.masthead-inner {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: center;
  gap: var(--space-5);
  min-height: var(--masthead-height);
  padding-top: var(--space-3);
  padding-bottom: var(--space-3);
}

.brand {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  color: var(--color-text);
  text-decoration: none;
}

.brand-glyph {
  color: var(--color-accent);
}

.brand-text {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
}

.brand-word {
  font-family: var(--font-display);
  font-size: var(--text-lg);
  font-weight: 600;
  letter-spacing: 0.22em;
}

.brand-zh {
  font-family: var(--font-display);
  font-size: var(--text-sm);
  color: var(--color-text-muted);
  letter-spacing: 0.2em;
}

.brand-tagline {
  padding-left: var(--space-3);
  border-left: 1px solid var(--color-rule);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  letter-spacing: 0.06em;
  color: var(--color-text-muted);
  white-space: nowrap;
}

.nav {
  display: flex;
  align-items: center;
  gap: var(--space-1);
}

.nav-link {
  padding: var(--space-2) var(--space-3);
  border-bottom: 2px solid transparent;
  color: var(--color-text-muted);
  font-size: var(--text-sm);
  text-decoration: none;
  white-space: nowrap;
}

.nav-link:hover {
  color: var(--color-text);
  text-decoration: none;
}

.nav-link--active {
  color: var(--color-text);
  border-bottom-color: var(--color-accent);
}

.account {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--space-2);
  min-width: 0;
}

.account-avatar {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2rem;
  height: 2rem;
  border: 1px solid var(--color-rule-strong);
  border-radius: var(--radius-xs);
  background-color: var(--color-bg);
  color: var(--color-text);
  font-family: var(--font-display);
  font-size: var(--text-sm);
  overflow: hidden;
  flex-shrink: 0;
}

.account-image {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.account-meta {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.account-name {
  max-width: 10rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--text-sm);
}

.account-role {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--color-accent-deep);
}

.account-logout {
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-rule-strong);
  border-radius: var(--radius-xs);
  color: var(--color-text-muted);
  font-size: var(--text-xs);
  white-space: nowrap;
}

.account-logout:hover {
  border-color: var(--color-accent);
  color: var(--color-accent-deep);
}

.maintenance {
  margin: var(--space-4) 0 0;
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-warning);
  border-radius: var(--radius-xs);
  background-color: var(--color-warning-bg);
  color: var(--color-warning);
  font-size: var(--text-sm);
}

.main {
  flex: 1;
  padding: var(--space-7) 0 var(--space-8);
}

.colophon {
  border-top: 1px solid var(--color-rule);
  padding: var(--space-4) 0;
  background-color: var(--color-surface);
}

.colophon-inner {
  display: flex;
  justify-content: space-between;
  gap: var(--space-4);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  letter-spacing: 0.06em;
  color: var(--color-text-muted);
}

@media (max-width: 1180px) {
  .brand-tagline {
    display: none;
  }
}

@media (max-width: 980px) {
  .masthead-inner {
    grid-template-columns: 1fr auto;
    grid-template-areas:
      'brand account'
      'nav nav';
    row-gap: var(--space-2);
  }

  .brand {
    grid-area: brand;
  }

  .account {
    grid-area: account;
  }

  .nav {
    grid-area: nav;
    overflow-x: auto;
    padding-bottom: var(--space-1);
    border-top: 1px solid var(--color-rule);
  }

  .nav-link {
    padding-top: var(--space-1);
    padding-bottom: var(--space-1);
  }

  .account-meta {
    display: none;
  }
}

@media (max-width: 720px) {
  .masthead {
    position: static;
  }

  .main {
    padding: var(--space-5) 0 var(--space-7);
  }

  .colophon-inner {
    flex-direction: column;
    gap: var(--space-1);
  }
}
</style>
