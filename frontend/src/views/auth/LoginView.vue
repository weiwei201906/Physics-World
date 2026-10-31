<script setup lang="ts">
import { ref } from 'vue'
import { RouterLink, useRoute, useRouter } from 'vue-router'
import { t } from '@/i18n'
import BrandMark from '@/components/BrandMark.vue'
import PhysicsGlyph from '@/components/PhysicsGlyph.vue'
import { useAuthStore } from '@/stores/auth'

const auth = useAuthStore()
const router = useRouter()
const route = useRoute()

const email = ref('')
const password = ref('')
const error = ref('')
const loading = ref(false)

function redirectTarget(): string {
  const value = route.query.redirect
  if (typeof value === 'string' && value.startsWith('/')) return value
  return '/'
}

async function handleSubmit(): Promise<void> {
  error.value = ''
  if (!email.value.trim()) {
    error.value = t('auth.emailRequired')
    return
  }
  if (!password.value) {
    error.value = t('auth.passwordTooShort', { min: 8 })
    return
  }
  loading.value = true
  try {
    await auth.login(email.value.trim(), password.value)
    await router.push(redirectTarget())
  } catch {
    error.value = t('auth.loginFailed')
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="auth-shell">
    <div class="auth-grid">
      <section class="auth-form-col">
        <RouterLink to="/" class="brand">
          <BrandMark :size="34" class="brand-glyph" />
          <span class="brand-text">
            <span class="brand-word">{{ t('brand.name') }}</span>
            <span class="brand-zh">{{ t('brand.nameZh') }}</span>
          </span>
        </RouterLink>

        <p class="eyebrow">{{ t('brand.tagline') }}</p>
        <h1 class="auth-title">{{ t('auth.loginTitle') }}</h1>
        <p class="auth-subtitle">{{ t('auth.loginSubtitle') }}</p>

        <form class="form auth-form" @submit.prevent="handleSubmit">
          <div class="field">
            <label class="field-label" for="email">{{ t('auth.email') }}</label>
            <input
              id="email"
              v-model="email"
              type="email"
              class="field-input"
              autocomplete="email"
              required
            />
          </div>

          <div class="field">
            <label class="field-label" for="password">{{ t('auth.password') }}</label>
            <input
              id="password"
              v-model="password"
              type="password"
              class="field-input"
              autocomplete="current-password"
              required
            />
            <span class="field-hint">{{ t('auth.passwordHint', { min: 8 }) }}</span>
          </div>

          <p v-if="error" class="form-error" role="alert">{{ error }}</p>

          <button type="submit" class="btn btn-accent btn-lg btn-block" :disabled="loading">
            {{ loading ? t('auth.signingIn') : t('auth.signIn') }}
          </button>
        </form>

        <p class="auth-footer">
          {{ t('auth.noAccount') }}
          <RouterLink to="/register">{{ t('auth.goRegister') }}</RouterLink>
        </p>
      </section>

      <aside class="auth-aside" aria-hidden="true">
        <div class="aside-rule" />
        <PhysicsGlyph name="atom" :size="88" class="aside-glyph" />
        <p class="aside-line">{{ t('auth.brandLine') }}</p>
        <div class="aside-notes">
          <p class="aside-note">{{ t('auth.notesTitle') }}</p>
          <ul class="note-list">
            <li>{{ t('auth.noteOne') }}</li>
            <li>{{ t('auth.noteTwo') }}</li>
            <li>{{ t('auth.noteThree') }}</li>
          </ul>
        </div>
      </aside>
    </div>
  </div>
</template>

<style scoped>
.auth-shell {
  min-height: 100vh;
  background-color: var(--color-bg);
  padding: var(--space-6) var(--space-5);
}

.auth-grid {
  display: grid;
  grid-template-columns: minmax(0, 5fr) minmax(0, 7fr);
  gap: var(--space-7);
  max-width: var(--container-max);
  margin: 0 auto;
}

.auth-form-col {
  display: flex;
  flex-direction: column;
  padding: var(--space-5) 0;
}

.brand {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin-bottom: var(--space-7);
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
  letter-spacing: 0.24em;
  font-weight: 600;
}

.brand-zh {
  font-family: var(--font-display);
  font-size: var(--text-sm);
  letter-spacing: 0.2em;
  color: var(--color-text-muted);
}

.auth-title {
  font-size: var(--text-3xl);
}

.auth-subtitle {
  margin: var(--space-2) 0 var(--space-6);
  color: var(--color-text-muted);
  font-size: var(--text-sm);
}

.auth-form {
  max-width: 24rem;
}

.auth-footer {
  margin-top: var(--space-5);
  padding-top: var(--space-4);
  border-top: 1px solid var(--color-rule);
  color: var(--color-text-muted);
  font-size: var(--text-sm);
}

.auth-aside {
  position: relative;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  padding: var(--space-6);
  border: 1px solid var(--color-rule);
  border-radius: var(--radius-sm);
  background-color: var(--color-surface);
  overflow: hidden;
}

.aside-rule {
  position: absolute;
  top: var(--space-6);
  left: var(--space-6);
  right: var(--space-6);
  height: 1px;
  background-color: var(--color-rule);
}

.aside-glyph {
  position: absolute;
  top: var(--space-7);
  right: var(--space-6);
  color: var(--color-accent);
  opacity: 0.9;
}

.aside-line {
  max-width: 24ch;
  margin: 0 0 var(--space-6);
  font-family: var(--font-display);
  font-size: var(--text-2xl);
  line-height: 1.35;
  color: var(--color-text);
}

.aside-notes {
  padding-top: var(--space-4);
  border-top: 1px solid var(--color-rule);
}

.aside-note {
  margin: 0 0 var(--space-2);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--color-text-muted);
}

.note-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}

@media (max-width: 960px) {
  .auth-grid {
    grid-template-columns: 1fr;
    gap: var(--space-5);
  }

  .auth-aside {
    min-height: 18rem;
  }

  .auth-shell {
    padding: var(--space-5) var(--space-4);
  }
}
</style>
