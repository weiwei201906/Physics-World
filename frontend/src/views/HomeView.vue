<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink } from 'vue-router'
import { t, percent } from '@/i18n'
import { fetchChapters, fetchMyRank } from '@/api/user'
import { useProfileStore } from '@/stores/profile'
import { useConfigStore } from '@/stores/config'
import type { Chapter, RankSummary } from '@/types'
import PhysicsGlyph from '@/components/PhysicsGlyph.vue'
import LoadingSkeleton from '@/components/LoadingSkeleton.vue'

const profile = useProfileStore()
const config = useConfigStore()

const chapters = ref<Chapter[]>([])
const rank = ref<RankSummary | null>(null)
const chaptersFailed = ref(false)
const rankFailed = ref(false)

const strongest = computed(() => {
  const list = profile.summary?.chapterStrengths ?? []
  if (list.length === 0) return null
  return [...list].sort((a, b) => b.strength - a.strength)[0]
})

const weakest = computed(() => {
  const list = profile.summary?.chapterStrengths ?? []
  if (list.length === 0) return null
  return [...list].sort((a, b) => a.strength - b.strength)[0]
})

const topChapters = computed(() => chapters.value.slice(0, 5))

const rankLabel = computed(() => {
  if (!rank.value || rank.value.rank === null) return t('home.rankNone')
  return t('leaderboard.myRankValue', { rank: rank.value.rank, score: rank.value.score })
})

async function loadChapters(): Promise<void> {
  try {
    chapters.value = await fetchChapters()
    chaptersFailed.value = false
  } catch {
    chaptersFailed.value = true
  }
}

async function loadRank(): Promise<void> {
  try {
    rank.value = await fetchMyRank('DAILY')
    rankFailed.value = false
  } catch {
    rankFailed.value = true
  }
}

onMounted(() => {
  void profile.loadSummary()
  void loadChapters()
  void loadRank()
})
</script>

<template>
  <div class="home stack-lg">
    <header class="page-head">
      <div>
        <p class="eyebrow">{{ t('brand.name') }}</p>
        <h1 class="page-title">{{ t('home.title') }}</h1>
        <p class="page-desc">{{ t('home.lead') }}</p>
      </div>
      <div class="head-actions">
        <RouterLink to="/practice" class="btn btn-accent">{{ t('home.startPractice') }}</RouterLink>
        <RouterLink to="/practice?mode=EXAM" class="btn btn-secondary">
          {{ t('home.startExam') }}
        </RouterLink>
      </div>
    </header>

    <div class="grid-12 overview">
      <section class="span-7 quick" aria-labelledby="quick-title">
        <div class="quick-head">
          <PhysicsGlyph name="wave" :size="40" class="quick-glyph" />
          <h2 id="quick-title" class="section-title">{{ t('home.quickStart') }}</h2>
        </div>
        <p class="quick-lead">{{ t('home.lead') }}</p>
        <div class="quick-actions">
          <RouterLink to="/practice" class="quick-link">
            <span class="quick-label">{{ t('practice.modePractice') }}</span>
            <span class="quick-note">{{ t('practice.modePracticeDesc') }}</span>
          </RouterLink>
          <RouterLink to="/practice?mode=EXAM" class="quick-link">
            <span class="quick-label">{{ t('practice.modeExam') }}</span>
            <span class="quick-note">{{ t('practice.modeExamDesc') }}</span>
          </RouterLink>
        </div>
      </section>

      <section class="span-5 ledger" aria-labelledby="ledger-title">
        <div class="ledger-head ledger-row">
          <h2 id="ledger-title" class="ledger-title">{{ t('home.ledgerTitle') }}</h2>
        </div>
        <LoadingSkeleton v-if="profile.loading" :lines="4" />
        <template v-else>
          <div class="ledger-row ledger-line">
            <span class="ledger-key">{{ t('home.answered') }}</span>
            <span class="ledger-value mono">{{ profile.summary?.totalAttempts ?? 0 }}</span>
          </div>
          <div class="ledger-row ledger-line">
            <span class="ledger-key">{{ t('home.correctRate') }}</span>
            <span class="ledger-value mono">
              {{ profile.summary ? percent(profile.summary.correctRate) : 0 }}%
            </span>
          </div>
          <div class="ledger-row ledger-line">
            <span class="ledger-key">{{ t('home.strongest') }}</span>
            <span class="ledger-value">{{ strongest?.chapterName ?? t('home.noData') }}</span>
          </div>
          <div class="ledger-row ledger-line">
            <span class="ledger-key">{{ t('home.weakest') }}</span>
            <span class="ledger-value">{{ weakest?.chapterName ?? t('home.noData') }}</span>
          </div>
          <div class="ledger-row ledger-line">
            <span class="ledger-key">{{ t('home.rankLabel') }}</span>
            <span class="ledger-value">{{ rankLabel }}</span>
          </div>
        </template>
        <p v-if="rankFailed" class="inline-note">{{ t('home.loadError') }}</p>
      </section>
    </div>

    <section aria-labelledby="chapters-title">
      <div class="section-head">
        <h2 id="chapters-title" class="section-title">
          {{ t('home.chapters') }}
          <small>{{ t('chapter.listDesc') }}</small>
        </h2>
        <RouterLink to="/chapters" class="text-link">{{ t('home.viewAll') }}</RouterLink>
      </div>

      <LoadingSkeleton v-if="!chapters.length && !chaptersFailed" :lines="3" />
      <p v-else-if="chaptersFailed" class="inline-error">{{ t('chapter.error') }}</p>
      <div v-else class="chapter-ledger ledger">
        <div class="ledger-row ledger-head chapter-row">
          <span>{{ t('chapter.index') }}</span>
          <span>{{ t('chapter.name') }}</span>
          <span>{{ t('chapter.questionCount') }}</span>
          <span>{{ t('chapter.openAction') }}</span>
        </div>
        <RouterLink
          v-for="(chapter, index) in topChapters"
          :key="chapter.id"
          :to="{ name: 'chapter', params: { chapterId: chapter.id } }"
          class="ledger-row chapter-row chapter-link"
        >
          <span class="mono">{{ (index + 1).toString().padStart(2, '0') }}</span>
          <span class="chapter-name">{{ chapter.name }}</span>
          <span class="mono">{{ t('chapter.count', { count: chapter.questionCount }) }}</span>
          <span class="text-link">{{ t('chapter.open') }}</span>
        </RouterLink>
      </div>
    </section>

    <div class="grid-12 lower">
      <section class="span-7 panel callout" aria-labelledby="recommended-title">
        <PhysicsGlyph name="prism" :size="36" class="callout-glyph" />
        <h2 id="recommended-title" class="section-title">{{ t('home.recommendedTitle') }}</h2>
        <p class="page-desc">{{ t('home.recommendedDesc') }}</p>
        <RouterLink to="/recommendation" class="btn btn-secondary">
          {{ t('home.viewRecommendation') }}
        </RouterLink>
      </section>

      <section class="span-5 panel callout" aria-labelledby="battle-title">
        <PhysicsGlyph name="vector" :size="36" class="callout-glyph" />
        <h2 id="battle-title" class="section-title">{{ t('home.battleTitle') }}</h2>
        <p class="page-desc">{{ t('home.battleDesc') }}</p>
        <RouterLink to="/battle" class="btn btn-secondary">{{ t('home.enterBattle') }}</RouterLink>
      </section>
    </div>

    <section v-if="config.isMaintenance" class="notice notice--warning" role="status">
      {{ t('home.maintenance', { message: config.maintenanceMessage }) }}
    </section>
  </div>
</template>

<style scoped>
.home {
  --local-gap: var(--space-7);
}

.stack-lg {
  display: flex;
  flex-direction: column;
  gap: var(--space-7);
}

.head-actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.overview {
  align-items: start;
}

.quick {
  padding: var(--space-6);
  border: 1px solid var(--color-rule);
  border-radius: var(--radius-sm);
  background-color: var(--color-surface);
}

.quick-head {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.quick-glyph {
  color: var(--color-accent);
}

.quick .section-title {
  flex: 1;
  border-bottom: none;
  padding-bottom: 0;
}

.quick-lead {
  max-width: 40ch;
  margin: var(--space-3) 0 var(--space-5);
  color: var(--color-text-muted);
  font-family: var(--font-display);
  font-size: var(--text-xl);
  line-height: 1.35;
}

.quick-actions {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-3);
}

.quick-link {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  padding: var(--space-3) var(--space-4);
  border: 1px solid var(--color-rule-strong);
  border-radius: var(--radius-xs);
  text-decoration: none;
  color: var(--color-text);
}

.quick-link:hover {
  border-color: var(--color-accent);
  text-decoration: none;
}

.quick-label {
  font-family: var(--font-display);
  font-size: var(--text-base);
  font-weight: 600;
}

.quick-note {
  font-size: var(--text-xs);
  color: var(--color-text-muted);
}

.ledger-head {
  background-color: var(--color-bg-deep);
}

.ledger-title {
  font-size: var(--text-sm);
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.ledger-line {
  grid-template-columns: minmax(6rem, 0.8fr) minmax(0, 1.2fr);
}

.ledger-key {
  color: var(--color-text-muted);
  font-size: var(--text-sm);
}

.ledger-value {
  text-align: right;
  font-size: var(--text-sm);
  overflow-wrap: anywhere;
}

.section-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-4);
  margin-bottom: var(--space-3);
}

.section-head .section-title {
  flex: 1;
}

.chapter-row {
  grid-template-columns: 3rem minmax(0, 1fr) 6rem 4rem;
}

.chapter-link {
  color: var(--color-text);
  text-decoration: none;
}

.chapter-link:hover {
  background-color: var(--color-bg-deep);
  text-decoration: none;
}

.chapter-name {
  font-family: var(--font-display);
  font-size: var(--text-base);
}

.lower {
  align-items: stretch;
}

.callout {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  align-items: flex-start;
}

.callout-glyph {
  color: var(--color-moss);
}

.callout .section-title {
  border-bottom: none;
  padding-bottom: 0;
}

.callout .btn {
  margin-top: auto;
}

.inline-note {
  margin: 0;
  padding: var(--space-2) var(--space-4);
  border-top: 1px solid var(--color-rule);
  font-size: var(--text-xs);
  color: var(--color-text-muted);
}

.inline-error {
  margin: 0;
  padding: var(--space-4);
  border: 1px solid var(--color-wrong);
  border-radius: var(--radius-xs);
  color: var(--color-wrong);
  font-size: var(--text-sm);
}

@media (max-width: 1080px) {
  .quick-actions {
    grid-template-columns: 1fr;
  }
}

@media (max-width: 720px) {
  .stack-lg {
    gap: var(--space-6);
  }

  .quick {
    padding: var(--space-5);
  }

  .chapter-row {
    grid-template-columns: 2.5rem minmax(0, 1fr) auto;
  }

  .chapter-row > :nth-child(3) {
    display: none;
  }
}
</style>
