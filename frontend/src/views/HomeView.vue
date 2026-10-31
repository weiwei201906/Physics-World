<script setup lang="ts">
import { onMounted } from 'vue'
import { useProfileStore } from '@/stores/profile'
import { useConfigStore } from '@/stores/config'

const profile = useProfileStore()
const config = useConfigStore()

onMounted(() => {
  void profile.loadSummary()
})

const quickActions = [
  {
    title: 'Start practice',
    desc: 'Work through a mixed set at your pace.',
    to: '/practice',
    accent: 'primary'
  },
  {
    title: 'Recommended for you',
    desc: 'Questions targeted at your weak chapters.',
    to: '/recommendation',
    accent: 'info'
  },
  {
    title: 'Live battle',
    desc: 'Match with another student and race for score.',
    to: '/battle',
    accent: 'warning'
  }
] as const
</script>

<template>
  <div class="home">
    <header class="home-header">
      <div>
        <p class="eyebrow">Dashboard</p>
        <h1>Physics, practiced deliberately.</h1>
      </div>
      <RouterLink to="/practice" class="btn btn-primary">
        Start practicing
      </RouterLink>
    </header>

    <section class="stats-grid">
      <article class="stat-card">
        <span class="stat-label">Questions answered</span>
        <span class="stat-value">{{ profile.summary?.totalAttempts ?? 0 }}</span>
      </article>
      <article class="stat-card">
        <span class="stat-label">Correct rate</span>
        <span class="stat-value">
          {{ profile.summary ? Math.round(profile.summary.correctRate * 100) : 0 }}%
        </span>
      </article>
      <article class="stat-card stat-card--wide">
        <span class="stat-label">Strongest chapter</span>
        <span class="stat-value">
          {{ profile.summary?.chapterStrengths[0]?.chapterName ?? 'No data yet' }}
        </span>
      </article>
    </section>

    <section class="actions">
      <h2>Jump back in</h2>
      <div class="action-grid">
        <RouterLink
          v-for="action in quickActions"
          :key="action.title"
          :to="action.to"
          class="action-card"
          :class="`action-card--${action.accent}`"
        >
          <span class="action-title">{{ action.title }}</span>
          <span class="action-desc">{{ action.desc }}</span>
        </RouterLink>
      </div>
    </section>

    <section v-if="config.isMaintenance" class="maintenance-banner" role="status">
      {{ config.maintenanceMessage || 'System maintenance in progress.' }}
    </section>
  </div>
</template>

<style scoped>
.home {
  display: flex;
  flex-direction: column;
  gap: var(--space-7);
}

.home-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-end;
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

.home-header h1 {
  margin: 0;
}

.stats-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: var(--space-4);
}

.stat-card {
  background-color: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  padding: var(--space-5);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.stat-card--wide {
  grid-column: span 1;
}

.stat-label {
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}

.stat-value {
  font-family: var(--font-display);
  font-size: var(--text-2xl);
  font-weight: 700;
  color: var(--color-text);
}

.actions h2 {
  margin-bottom: var(--space-4);
}

.action-grid {
  display: grid;
  grid-template-columns: 1.2fr 1fr 1fr;
  gap: var(--space-4);
}

.action-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-5);
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border);
  background-color: var(--color-surface);
  text-decoration: none;
  transition: border-color var(--duration-fast) var(--ease-out),
              transform var(--duration-fast) var(--ease-out);
}

.action-card:hover {
  border-color: var(--color-border-strong);
  transform: translateY(-2px);
  text-decoration: none;
}

.action-title {
  font-family: var(--font-display);
  font-weight: 600;
  font-size: var(--text-lg);
  color: var(--color-text);
}

.action-desc {
  font-size: var(--text-sm);
  color: var(--color-text-muted);
  line-height: 1.5;
}

.action-card--primary {
  border-left: 3px solid var(--color-primary-600);
}
.action-card--info {
  border-left: 3px solid var(--color-info);
}
.action-card--warning {
  border-left: 3px solid var(--color-warning);
}

.maintenance-banner {
  padding: var(--space-4);
  background-color: var(--color-warning-bg);
  color: var(--color-warning);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
}

@media (max-width: 768px) {
  .home-header {
    flex-direction: column;
    align-items: flex-start;
  }
  .stats-grid {
    grid-template-columns: 1fr;
  }
  .action-grid {
    grid-template-columns: 1fr;
  }
}
</style>
