<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { t } from '@/i18n'
import { BATTLE_FRAME, cancelMatchFrame, findMatchFrame } from '@/api/battle'
import { useBattleWebSocket } from '@/composables/useBattleWebSocket'
import { useToast } from '@/composables/useToast'
import { useAuthStore } from '@/stores/auth'
import { useConfigStore } from '@/stores/config'
import PhysicsGlyph from '@/components/PhysicsGlyph.vue'
import type { BattleMode } from '@/types'

const router = useRouter()
const auth = useAuthStore()
const config = useConfigStore()
const { showToast } = useToast()

const mode = ref<BattleMode>('1v1')
const matching = ref(false)
const matchError = ref('')

const { connected, refreshing, connect, send, onMessage, disconnect } = useBattleWebSocket(
  () => auth.accessToken,
  {
    refreshToken: () => auth.forceRefresh(),
    onRefreshFailed: () => {
      showToast(t('battle.sessionExpired'), 'error')
      matching.value = false
      auth.degradeToLogin()
    },
    onRevoked: () => auth.handleRevoked()
  }
)

onMessage((data) => {
  const frame = data as { type?: string; message?: string; roomId?: string }
  if (frame.type === BATTLE_FRAME.MATCH_FOUND && frame.roomId) {
    matching.value = false
    void router.push({ name: 'battle-room', params: { roomId: frame.roomId } })
    return
  }
  if (frame.type === BATTLE_FRAME.ERROR) {
    matching.value = false
    matchError.value = frame.message || t('battle.matchError')
  }
})

watch(connected, (isConnected) => {
  if (isConnected && matching.value) {
    send(findMatchFrame(mode.value))
  }
})

onMounted(() => {
  if (!config.config.features.battleEnabled) return
  connect()
})

function findMatch(): void {
  if (matching.value) return
  matchError.value = ''
  matching.value = true
  connect()
}

function cancelMatch(): void {
  matching.value = false
  if (connected.value) {
    send(cancelMatchFrame())
  }
  disconnect()
}

function selectMode(next: BattleMode): void {
  if (matching.value) return
  mode.value = next
}
</script>

<template>
  <div class="lobby">
    <header class="page-head">
      <div>
        <p class="eyebrow">{{ t('battle.matchMode') }}</p>
        <h1 class="page-title">{{ t('battle.title') }}</h1>
        <p class="page-desc">{{ t('battle.desc') }}</p>
      </div>
      <span class="badge" :class="{ 'badge-correct': connected }">
        {{ connected ? t('battle.connected') : t('battle.disconnected') }}
      </span>
    </header>

    <section v-if="!config.config.features.battleEnabled" class="notice notice--warning">
      {{ t('battle.disabled') }}
    </section>

    <div v-else class="grid-12">
      <section class="span-8 match-panel">
        <PhysicsGlyph name="vector" :size="40" class="lobby-glyph" />
        <div class="mode-options" role="group" :aria-label="t('battle.matchMode')">
          <button
            type="button"
            class="mode-option"
            :class="{ 'mode-option--active': mode === '1v1' }"
            :aria-pressed="mode === '1v1'"
            @click="selectMode('1v1')"
          >
            <span class="mode-name">{{ t('battle.mode1v1') }}</span>
            <span class="mode-note">{{ t('battle.mode1v1Desc') }}</span>
          </button>
          <button
            type="button"
            class="mode-option"
            :class="{ 'mode-option--active': mode === 'squad' }"
            :aria-pressed="mode === 'squad'"
            @click="selectMode('squad')"
          >
            <span class="mode-name">{{ t('battle.squad') }}</span>
            <span class="mode-note">{{ t('battle.squadDesc') }}</span>
          </button>
        </div>

        <div class="lobby-actions">
          <button v-if="!matching" class="btn btn-accent btn-lg" @click="findMatch">
            {{ t('battle.find') }}
          </button>
          <button v-else class="btn btn-secondary btn-lg" @click="cancelMatch">
            {{ t('battle.cancel') }}
          </button>
          <p v-if="matching" class="match-state" role="status">
            <span class="pulse" aria-hidden="true" />
            {{ t('battle.finding') }}
          </p>
          <p v-if="refreshing" class="match-state">{{ t('battle.refreshHint') }}</p>
        </div>
      </section>

      <aside class="span-4 lobby-aside">
        <h2 class="section-title">{{ t('battle.status') }}</h2>
        <div class="aside-rows">
          <div class="aside-row">
            <span class="label">{{ t('battle.status') }}</span>
            <span class="aside-value">
              {{ connected ? t('battle.connected') : t('battle.disconnected') }}
            </span>
          </div>
          <div class="aside-row">
            <span class="label">{{ t('battle.players') }}</span>
            <span class="aside-value">{{ t('common.items', { count: 1 }) }}</span>
          </div>
          <div class="aside-row">
            <span class="label">{{ t('battle.matchMode') }}</span>
            <span class="aside-value">{{ mode === '1v1' ? t('battle.mode1v1') : t('battle.squad') }}</span>
          </div>
        </div>
        <p class="aside-note">{{ t('battle.cancelHint') }}</p>
      </aside>
    </div>

    <p v-if="matchError" class="form-error" role="alert">{{ matchError }}</p>
  </div>
</template>

<style scoped>
.lobby {
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
}

.match-panel {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  padding: var(--space-6);
  border: 1px solid var(--color-rule);
  border-radius: var(--radius-sm);
  background-color: var(--color-surface);
}

.lobby-glyph {
  color: var(--color-accent);
}

.mode-options {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-3);
}

.mode-option {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  padding: var(--space-4);
  border: 1px solid var(--color-rule-strong);
  border-radius: var(--radius-xs);
  background-color: var(--color-bg);
  text-align: left;
  color: var(--color-text);
}

.mode-option:hover {
  border-color: var(--color-text);
}

.mode-option--active {
  border-color: var(--color-accent);
}

.mode-option--active .mode-name {
  color: var(--color-accent-deep);
}

.mode-name {
  font-family: var(--font-display);
  font-size: var(--text-lg);
  font-weight: 600;
}

.mode-note {
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}

.lobby-actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
  padding-top: var(--space-4);
  border-top: 1px solid var(--color-rule);
}

.match-state {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}

.pulse {
  width: 6px;
  height: 6px;
  background-color: var(--color-accent);
  animation: pulse-fade 1.4s ease-in-out infinite;
}

@keyframes pulse-fade {
  0%,
  100% { opacity: 1; }
  50% { opacity: 0.25; }
}

.lobby-aside {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-5);
  border: 1px solid var(--color-rule);
  border-radius: var(--radius-sm);
}

.lobby-aside .section-title {
  border-bottom: 1px solid var(--color-rule);
  padding-bottom: var(--space-2);
  font-size: var(--text-lg);
}

.aside-rows {
  display: flex;
  flex-direction: column;
}

.aside-row {
  display: flex;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-2) 0;
  border-bottom: 1px solid var(--color-rule);
  font-size: var(--text-sm);
}

.aside-value {
  color: var(--color-text);
}

.aside-note {
  margin: 0;
  font-size: var(--text-xs);
  color: var(--color-text-muted);
}

@media (max-width: 900px) {
  .mode-options {
    grid-template-columns: 1fr;
  }
}

@media (max-width: 720px) {
  .match-panel {
    padding: var(--space-4);
  }

  .lobby-actions .btn {
    width: 100%;
  }
}
</style>
