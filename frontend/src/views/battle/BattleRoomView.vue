<script setup lang="ts">
import { computed, onMounted, onUnmounted, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { useBattleWebSocket } from '@/composables/useBattleWebSocket'
import { useBattleRoomState } from '@/composables/useBattleRoomState'
import { joinRoomFrame } from '@/api/battle'
import { t } from '@/i18n'

const props = defineProps<{ roomId: string }>()
const router = useRouter()
const auth = useAuthStore()

const { connected, connect, send, onMessage, disconnect } = useBattleWebSocket(
  () => auth.accessToken
)

// All battle UI state and transitions live in the state machine. The view only
// renders `phase` + `context` and forwards user input to `submit`.
const { phase, context, transition, submit, confirmEnd, answeredForIndex } = useBattleRoomState(send)

const myUserId = computed(() => auth.user?.id ?? '')

const battleOutcome = computed<'win' | 'lose' | 'draw'>(() => {
  if (!context.winnerId) return 'draw'
  return context.winnerId === myUserId.value ? 'win' : 'lose'
})

const sortedPlayers = computed(() =>
  [...context.players].sort((a, b) => a.rank - b.rank || b.score - a.score)
)

function outcomeLabel(): string {
  switch (battleOutcome.value) {
    case 'win':
      return t('battle.resultWin')
    case 'lose':
      return t('battle.resultLose')
    default:
      return t('battle.resultDraw')
  }
}

function optionClass(optionId: string): string {
  const result = answeredForIndex.value
  if (!result) {
    return context.selectedAnswer === optionId ? 'option--selected' : ''
  }
  if (optionId === result.correctOptionId) return 'option--correct'
  if (optionId === context.selectedAnswer && !result.correct) return 'option--wrong'
  return ''
}

watch(connected, (isConnected) => {
  if (isConnected) {
    send(joinRoomFrame(props.roomId))
  }
})

onMounted(() => {
  onMessage((data) => transition(data as Record<string, unknown>))
  connect()
})

onUnmounted(() => {
  disconnect()
})

function leave(): void {
  void router.push({ name: 'battle' })
}
</script>

<template>
  <div class="battle-room">
    <header class="room-head">
      <span class="room-id">{{ t('battle.room', { id: roomId }) }}</span>
      <span class="conn" :class="{ 'conn--ok': connected }">
        {{ connected ? t('battle.connected') : t('battle.connectionLost') }}
      </span>
    </header>

    <section class="players">
      <div v-for="p in context.players" :key="p.userId" class="player">
        <span class="player-name">{{ p.nickname }}</span>
        <span class="player-score">{{ p.score }}</span>
        <span v-if="p.correctCount != null" class="player-correct">
          {{ t('battle.correctShort', { count: p.correctCount }) }}
        </span>
      </div>
    </section>

    <div v-if="phase === 'waiting'" class="state">
      {{ t('battle.waiting') }}
    </div>

    <div v-else-if="phase === 'answering' || phase === 'revealed'" class="question-block">
      <p v-if="context.currentQuestion" class="stem">{{ context.currentQuestion.stem }}</p>
      <div v-if="context.currentQuestion" class="options">
        <button
          v-for="opt in context.currentQuestion.options"
          :key="opt.id"
          class="option"
          :class="optionClass(opt.id)"
          :disabled="!!context.selectedAnswer"
          @click="submit(opt.id)"
        >
          {{ opt.text }}
        </button>
      </div>
      <div
        v-if="answeredForIndex"
        class="feedback"
        :class="answeredForIndex.correct ? 'feedback--correct' : 'feedback--wrong'"
      >
        <p v-if="answeredForIndex.correct">
          {{ t('battle.answerCorrect', { score: answeredForIndex.score }) }}
        </p>
        <p v-else>
          {{ t('battle.answerWrong', { score: answeredForIndex.score }) }}
        </p>
        <p
          v-if="!answeredForIndex.correct && answeredForIndex.correctOptionId"
          class="feedback-correct"
        >
          {{ t('quiz.correctAnswer', { label: answeredForIndex.correctOptionId }) }}
        </p>
        <p v-if="answeredForIndex.explanation" class="feedback-explanation">
          {{ t('quiz.explanation') }}：{{ answeredForIndex.explanation }}
        </p>
      </div>
      <button
        v-if="context.pendingFinish"
        class="btn btn-primary"
        @click="confirmEnd"
      >
        {{ t('battle.endBattle') }}
      </button>
    </div>

    <div v-else-if="phase === 'finished'" class="state">
      <h2 class="outcome" :class="`outcome--${battleOutcome}`">{{ outcomeLabel() }}</h2>
      <table class="summary">
        <thead>
          <tr>
            <th>{{ t('battle.rank') }}</th>
            <th>{{ t('battle.players') }}</th>
            <th>{{ t('battle.score') }}</th>
            <th>{{ t('battle.correctCount') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="p in sortedPlayers"
            :key="p.userId"
            :class="{ 'row--me': p.userId === myUserId }"
          >
            <td>{{ p.rank }}</td>
            <td>
              {{ p.nickname }}
              <span v-if="p.userId === myUserId" class="me-tag">({{ t('battle.you') }})</span>
            </td>
            <td>{{ p.score }}</td>
            <td>{{ p.correctCount }}</td>
          </tr>
        </tbody>
      </table>
      <button class="btn btn-primary" @click="leave">{{ t('battle.backToLobby') }}</button>
    </div>
  </div>
</template>

<style scoped>
.battle-room {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  max-width: 720px;
  margin: 0 auto;
}

.room-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.room-id {
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}

.conn {
  font-size: var(--text-sm);
  color: var(--color-text-faint);
}
.conn--ok { color: var(--color-correct); }

.players {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-3);
}

.player {
  display: flex;
  justify-content: space-between;
  padding: var(--space-3) var(--space-4);
  background-color: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
}

.player-name { font-weight: 500; }
.player-score {
  font-family: var(--font-display);
  font-weight: 700;
  color: var(--color-primary-700);
}

.state {
  padding: var(--space-8);
  text-align: center;
  color: var(--color-text-muted);
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  align-items: center;
}

.question-block {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.stem {
  font-family: var(--font-display);
  font-size: var(--text-xl);
  font-weight: 600;
  margin: 0;
}

.options {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.option {
  padding: var(--space-3) var(--space-4);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background-color: var(--color-surface);
  text-align: left;
  transition: border-color var(--duration-fast) var(--ease-out);
}

.option:hover:not(:disabled) {
  border-color: var(--color-border-strong);
}

.option--selected {
  border-color: var(--color-primary-600);
  background-color: var(--color-primary-50);
}

.option--correct {
  border-color: var(--color-correct);
  background-color: var(--color-correct-bg, #e8f5e9);
}

.option--wrong {
  border-color: var(--color-wrong, #d32f2f);
  background-color: var(--color-wrong-bg, #ffebee);
}

.feedback {
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
}
.feedback--correct {
  background-color: var(--color-correct-bg, #e8f5e9);
  color: var(--color-correct, #2e7d32);
}
.feedback--wrong {
  background-color: var(--color-wrong-bg, #ffebee);
  color: var(--color-wrong, #c62828);
}
.feedback-correct {
  margin: var(--space-1) 0 0;
  font-weight: 600;
}
.feedback-explanation {
  margin: var(--space-2) 0 0;
  color: var(--color-text);
}

.player-correct {
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}

.outcome {
  margin: 0;
  font-family: var(--font-display);
  font-size: var(--text-2xl);
  font-weight: 700;
}
.outcome--win { color: var(--color-correct, #2e7d32); }
.outcome--lose { color: var(--color-wrong, #c62828); }
.outcome--draw { color: var(--color-text-muted); }

.summary {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--text-sm);
}
.summary th,
.summary td {
  padding: var(--space-2) var(--space-3);
  text-align: left;
  border-bottom: 1px solid var(--color-border);
}
.summary th {
  color: var(--color-text-muted);
  font-weight: 500;
}
.row--me {
  background-color: var(--color-primary-50, #eef2ff);
}
.me-tag {
  color: var(--color-primary-600);
  font-weight: 600;
}
</style>
