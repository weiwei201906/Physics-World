<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import { fetchLeaderboard } from '@/api/user'
import type { LeaderboardEntry, LeaderboardPeriod } from '@/types'

const periods: LeaderboardPeriod[] = ['DAILY', 'WEEKLY', 'ALL']
const period = ref<LeaderboardPeriod>('DAILY')
const entries = ref<LeaderboardEntry[]>([])
const loading = ref(true)

async function load(): Promise<void> {
  loading.value = true
  try {
    const page = await fetchLeaderboard(period.value, 1, 50)
    entries.value = page.entries
  } finally {
    loading.value = false
  }
}

onMounted(load)
watch(period, load)
</script>

<template>
  <div class="leaderboard">
    <header class="page-head">
      <p class="eyebrow">Leaderboard</p>
      <h1>Top performers</h1>
    </header>

    <div class="period-tabs" role="tablist">
      <button
        v-for="p in periods"
        :key="p"
        class="period-tab"
        :class="{ 'period-tab--active': period === p }"
        :aria-selected="period === p"
        role="tab"
        @click="period = p"
      >
        {{ p.charAt(0) + p.slice(1).toLowerCase() }}
      </button>
    </div>

    <div v-if="loading" class="loading">Loading...</div>

    <ol v-else class="rank-list">
      <li v-for="entry in entries" :key="entry.userId" class="rank-row">
        <span class="rank">{{ entry.rank }}</span>
        <div class="rank-user">
          <span class="rank-name">{{ entry.nickname }}</span>
        </div>
        <span class="rank-score">{{ entry.score }}</span>
      </li>
    </ol>
  </div>
</template>

<style scoped>
.leaderboard {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
}

.eyebrow {
  font-size: var(--text-sm);
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--color-primary-700);
  font-weight: 600;
  margin: 0 0 var(--space-2);
}

.period-tabs {
  display: flex;
  gap: var(--space-1);
  background-color: var(--color-bg-muted);
  padding: 4px;
  border-radius: var(--radius-md);
  width: fit-content;
}

.period-tab {
  padding: var(--space-1) var(--space-3);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  font-weight: 500;
  color: var(--color-text-muted);
  transition: background-color var(--duration-fast) var(--ease-out),
              color var(--duration-fast) var(--ease-out);
}

.period-tab--active {
  background-color: var(--color-surface);
  color: var(--color-text);
  box-shadow: var(--shadow-sm);
}

.loading {
  padding: var(--space-7);
  text-align: center;
  color: var(--color-text-muted);
}

.rank-list {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.rank-row {
  display: grid;
  grid-template-columns: 48px 1fr 80px;
  align-items: center;
  gap: var(--space-4);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-md);
  background-color: var(--color-surface);
  border: 1px solid var(--color-border);
}

.rank-row:nth-child(1) .rank { color: #B45309; }
.rank-row:nth-child(2) .rank { color: #475569; }
.rank-row:nth-child(3) .rank { color: #92400E; }

.rank {
  font-family: var(--font-display);
  font-weight: 700;
  font-size: var(--text-lg);
  color: var(--color-text-muted);
  text-align: center;
}

.rank-name {
  font-weight: 500;
  color: var(--color-text);
}

.rank-score {
  font-family: var(--font-display);
  font-weight: 600;
  text-align: right;
  color: var(--color-text);
  font-variant-numeric: tabular-nums;
}
</style>
