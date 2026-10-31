<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { useBattleWebSocket } from '@/composables/useBattleWebSocket'

const props = defineProps<{ roomId: string }>()
const router = useRouter()
const auth = useAuthStore()

const { connected, connect, send, onMessage, disconnect } = useBattleWebSocket(
  () => auth.accessToken
)

const status = ref<'waiting' | 'playing' | 'finished'>('waiting')
const players = ref<Array<{ userId: string; nickname: string; score: number }>>([])
const currentQuestion = ref<{ stem: string; options: Array<{ id: string; text: string }> } | null>(null)
const selectedAnswer = ref<string | null>(null)

onMounted(() => {
  connect()
  send({ type: 'join_room', roomId: props.roomId })

  onMessage((data) => {
    const msg = data as Record<string, unknown>
    if (msg.type === 'room_state') {
      players.value = msg.players as typeof players.value
      status.value = msg.status as typeof status.value
    }
    if (msg.type === 'question') {
      currentQuestion.value = msg.question as typeof currentQuestion.value
      status.value = 'playing'
    }
    if (msg.type === 'game_over') {
      status.value = 'finished'
    }
  })
})

onUnmounted(() => {
  disconnect()
})

function submit(optionId: string): void {
  selectedAnswer.value = optionId
  send({ type: 'answer', optionId })
}

function leave(): void {
  void router.push({ name: 'battle' })
}
</script>

<template>
  <div class="battle-room">
    <header class="room-head">
      <span class="room-id">Room {{ roomId }}</span>
      <span class="conn" :class="{ 'conn--ok': connected }">
        {{ connected ? 'Connected' : 'Reconnecting...' }}
      </span>
    </header>

    <section class="players">
      <div v-for="p in players" :key="p.userId" class="player">
        <span class="player-name">{{ p.nickname }}</span>
        <span class="player-score">{{ p.score }}</span>
      </div>
    </section>

    <div v-if="status === 'waiting'" class="state">
      Waiting for an opponent...
    </div>

    <div v-else-if="status === 'playing' && currentQuestion" class="question-block">
      <p class="stem">{{ currentQuestion.stem }}</p>
      <div class="options">
        <button
          v-for="opt in currentQuestion.options"
          :key="opt.id"
          class="option"
          :class="{ 'option--selected': selectedAnswer === opt.id }"
          :disabled="!!selectedAnswer"
          @click="submit(opt.id)"
        >
          {{ opt.text }}
        </button>
      </div>
    </div>

    <div v-else-if="status === 'finished'" class="state">
      Battle finished.
      <button class="btn btn-primary" @click="leave">Back to lobby</button>
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
</style>
