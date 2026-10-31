<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useProfileStore } from '@/stores/profile'
import { useAuthStore } from '@/stores/auth'
import { getBattleHistory } from '@/api/battle'
import { t } from '@/i18n'
import type { BattleHistoryEntry, BattleResultTag } from '@/types'

const profile = useProfileStore()
const auth = useAuthStore()

const history = ref<BattleHistoryEntry[]>([])
const historyLoading = ref(false)
const historyError = ref(false)

onMounted(() => {
  void profile.loadSummary()
  void loadHistory()
})

async function loadHistory(): Promise<void> {
  historyLoading.value = true
  historyError.value = false
  try {
    history.value = await getBattleHistory()
  } catch {
    historyError.value = true
  } finally {
    historyLoading.value = false
  }
}

function resultLabel(result: BattleResultTag): string {
  switch (result) {
    case 'win':
      return t('battle.resultWin')
    case 'lose':
      return t('battle.resultLose')
    default:
      return t('battle.resultDraw')
  }
}

function formatTime(iso: string): string {
  const date = new Date(iso)
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString()
}
</script>

<template>
  <div class="profile">
    <header class="profile-head">
      <div class="avatar">{{ auth.user?.nickname?.charAt(0).toUpperCase() }}</div>
      <div>
        <h1>{{ auth.user?.nickname }}</h1>
        <p class="email">{{ auth.user?.email }}</p>
      </div>
    </header>

    <section class="summary-stats">
      <div class="stat">
        <span class="stat-value">{{ profile.summary?.totalAttempts ?? 0 }}</span>
        <span class="stat-label">Questions answered</span>
      </div>
      <div class="stat">
        <span class="stat-value">
          {{ profile.summary ? Math.round(profile.summary.correctRate * 100) : 0 }}%
        </span>
        <span class="stat-label">Correct rate</span>
      </div>
    </section>

    <section class="strengths">
      <h2>Chapter strengths</h2>
      <div v-if="profile.loading" class="loading">Loading...</div>
      <div v-else-if="!profile.summary?.chapterStrengths.length" class="empty">
        No data yet. Answer some questions to see your strengths.
      </div>
      <div v-else class="strength-list">
        <div v-for="s in profile.summary.chapterStrengths" :key="s.chapterId" class="strength-item">
          <div class="strength-head">
            <span class="strength-name">{{ s.chapterName }}</span>
            <span class="strength-value">{{ Math.round(s.strength * 100) }}%</span>
          </div>
          <div class="strength-bar">
            <div class="strength-fill" :style="{ width: `${s.strength * 100}%` }" />
          </div>
        </div>
      </div>
    </section>

    <section class="battle-history">
      <h2>{{ t('profile.battleHistory') }}</h2>
      <div v-if="historyLoading" class="loading">{{ t('profile.battleLoading') }}</div>
      <div v-else-if="historyError" class="empty">{{ t('profile.battleError') }}</div>
      <div v-else-if="!history.length" class="empty">{{ t('profile.noBattles') }}</div>
      <div v-else class="history-list">
        <div v-for="entry in history" :key="entry.battleId" class="history-item">
          <div class="history-head">
            <span class="result" :class="`result--${entry.result}`">{{ resultLabel(entry.result) }}</span>
            <span class="history-mode">{{ entry.mode }}</span>
            <span class="history-time">{{ formatTime(entry.finishedAt) }}</span>
          </div>
          <div class="history-scores">
            <span class="score-me">{{ t('battle.you') }}: {{ entry.myScore }} ({{ entry.myCorrect }}/{{ entry.questionCount }})</span>
            <span v-if="entry.opponentNickname" class="score-opp">
              {{ entry.opponentNickname }}: {{ entry.opponentScore }} ({{ entry.opponentCorrect }}/{{ entry.questionCount }})
            </span>
          </div>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.profile {
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
  max-width: 720px;
}

.profile-head {
  display: flex;
  align-items: center;
  gap: var(--space-4);
}

.avatar {
  width: 64px;
  height: 64px;
  border-radius: 50%;
  background-color: var(--color-primary-700);
  color: var(--color-text-on-primary);
  display: flex;
  align-items: center;
  justify-content: center;
  font-family: var(--font-display);
  font-weight: 700;
  font-size: var(--text-2xl);
}

.email {
  color: var(--color-text-muted);
  margin: var(--space-1) 0 0;
}

.summary-stats {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-4);
}

.stat {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  padding: var(--space-4);
  background-color: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
}

.stat-value {
  font-family: var(--font-display);
  font-size: var(--text-2xl);
  font-weight: 700;
  color: var(--color-text);
}

.stat-label {
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}

.strengths h2 {
  margin-bottom: var(--space-4);
}

.loading,
.empty {
  padding: var(--space-5);
  text-align: center;
  color: var(--color-text-muted);
}

.strength-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.strength-item {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.strength-head {
  display: flex;
  justify-content: space-between;
}

.strength-name {
  font-weight: 500;
  color: var(--color-text);
}

.strength-value {
  font-size: var(--text-sm);
  color: var(--color-text-muted);
  font-variant-numeric: tabular-nums;
}

.strength-bar {
  height: 6px;
  background-color: var(--color-bg-muted);
  border-radius: 3px;
  overflow: hidden;
}

.strength-fill {
  height: 100%;
  background-color: var(--color-primary-600);
  border-radius: 3px;
  transition: width var(--duration-base) var(--ease-out);
}

.battle-history h2 {
  margin-bottom: var(--space-4);
}

.history-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.history-item {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  background-color: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
}

.history-head {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.result {
  font-weight: 600;
}
.result--win { color: var(--color-correct, #2e7d32); }
.result--lose { color: var(--color-wrong, #c62828); }
.result--draw { color: var(--color-text-muted); }

.history-mode {
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}

.history-time {
  margin-left: auto;
  font-size: var(--text-sm);
  color: var(--color-text-faint);
}

.history-scores {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  font-size: var(--text-sm);
}

.score-me {
  color: var(--color-primary-700);
  font-weight: 500;
}

.score-opp {
  color: var(--color-text-muted);
}
</style>
