<script setup lang="ts">
import { ref } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useAuthStore } from '@/stores/auth'

const auth = useAuthStore()
const router = useRouter()
const route = useRoute()

const email = ref('')
const password = ref('')
const error = ref('')
const loading = ref(false)

async function handleSubmit(): Promise<void> {
  error.value = ''
  loading.value = true
  try {
    await auth.login(email.value, password.value)
    const redirect = (route.query.redirect as string) ?? '/'
    await router.push(redirect)
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Sign in failed'
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="auth-page">
    <div class="auth-panel">
      <div class="auth-brand">
        <svg class="brand-mark" viewBox="0 0 32 32" fill="none" aria-hidden="true">
          <circle cx="16" cy="16" r="8" stroke="currentColor" stroke-width="2" />
          <ellipse cx="16" cy="16" rx="12" ry="4.5" stroke="currentColor" stroke-width="1.5" transform="rotate(30 16 16)" />
          <ellipse cx="16" cy="16" rx="12" ry="4.5" stroke="currentColor" stroke-width="1.5" transform="rotate(-30 16 16)" />
          <circle cx="16" cy="16" r="2" fill="currentColor" />
        </svg>
        <span>PhysLab</span>
      </div>

      <h1>Welcome back</h1>
      <p class="subtitle">Sign in to continue your physics practice.</p>

      <form class="form" @submit.prevent="handleSubmit">
        <div class="field">
          <label class="field-label" for="email">Email</label>
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
          <label class="field-label" for="password">Password</label>
          <input
            id="password"
            v-model="password"
            type="password"
            class="field-input"
            autocomplete="current-password"
            required
          />
        </div>

        <p v-if="error" class="form-error" role="alert">{{ error }}</p>

        <button type="submit" class="btn btn-primary btn-block" :disabled="loading">
          {{ loading ? 'Signing in...' : 'Sign in' }}
        </button>
      </form>

      <p class="auth-footer">
        No account?
        <RouterLink to="/register">Create one</RouterLink>
      </p>
    </div>

    <div class="auth-visual" aria-hidden="true">
      <div class="visual-grid">
        <div v-for="i in 8" :key="i" class="grid-line" :style="{ top: `${i * 12}%` }" />
        <div v-for="i in 8" :key="'v'+i" class="grid-line-vertical" :style="{ left: `${i * 12}%` }" />
      </div>
      <div class="visual-copy">
        <h2>Practice physics like a scientist.</h2>
        <p>Chapter drills, real-time battles, and recommendations that adapt to how you learn.</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.auth-page {
  display: grid;
  grid-template-columns: 1fr 1fr;
  min-height: 100vh;
}

.auth-panel {
  display: flex;
  flex-direction: column;
  justify-content: center;
  padding: var(--space-8) var(--space-7);
  max-width: 480px;
  margin: 0 auto;
  width: 100%;
}

.auth-brand {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-bottom: var(--space-7);
  font-family: var(--font-display);
  font-weight: 700;
  font-size: var(--text-xl);
  color: var(--color-primary-700);
}

.brand-mark {
  width: 32px;
  height: 32px;
}

.subtitle {
  color: var(--color-text-muted);
  margin: var(--space-2) 0 var(--space-6);
}

.form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.form-error {
  color: var(--color-wrong);
  font-size: var(--text-sm);
  margin: 0;
}

.auth-footer {
  margin-top: var(--space-5);
  font-size: var(--text-sm);
  color: var(--color-text-muted);
  text-align: center;
}

.auth-visual {
  position: relative;
  background-color: var(--color-primary-800);
  color: var(--color-text-on-primary);
  overflow: hidden;
  display: flex;
  align-items: flex-end;
  padding: var(--space-8);
}

.visual-grid {
  position: absolute;
  inset: 0;
  opacity: 0.15;
}

.grid-line {
  position: absolute;
  left: 0;
  right: 0;
  height: 1px;
  background: var(--color-text-on-primary);
}

.grid-line-vertical {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 1px;
  background: var(--color-text-on-primary);
}

.visual-copy {
  position: relative;
  z-index: 1;
  max-width: 420px;
}

.visual-copy h2 {
  color: var(--color-text-on-primary);
  margin-bottom: var(--space-3);
}

.visual-copy p {
  color: rgba(248, 250, 252, 0.8);
  margin: 0;
  line-height: 1.7;
}

@media (max-width: 900px) {
  .auth-page {
    grid-template-columns: 1fr;
  }
  .auth-visual {
    display: none;
  }
}
</style>
