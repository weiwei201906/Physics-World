import { computed, getCurrentInstance, onUnmounted, ref } from 'vue'

export interface ExamTimerOptions {
  startedAt: string
  durationSec: number
  warningSec: number
  onExpire?: () => void
  onWarning?: () => void
}

export function computeRemainingSeconds(
  startedAtMs: number,
  durationSec: number,
  nowMs: number
): number {
  const deadline = startedAtMs + durationSec * 1000
  return Math.max(0, Math.ceil((deadline - nowMs) / 1000))
}

/**
 * Exam countdown anchored to the server-provided session start time, so a page
 * refresh resumes the same remaining time instead of restarting the clock.
 */
export function useExamTimer(options: ExamTimerOptions, now: () => number = () => Date.now()) {
  const startedAtMs = Date.parse(options.startedAt)
  const anchor = Number.isFinite(startedAtMs) ? startedAtMs : now()
  const durationSec = Math.max(0, options.durationSec)
  const warningSec = Math.max(0, options.warningSec)

  const remaining = ref(computeRemainingSeconds(anchor, durationSec, now()))
  const expired = ref(remaining.value === 0)
  let interval: ReturnType<typeof setInterval> | null = null
  let warned = false
  let expiredNotified = false

  function tick(): void {
    const next = computeRemainingSeconds(anchor, durationSec, now())
    remaining.value = next
    if (!warned && next > 0 && next <= warningSec) {
      warned = true
      options.onWarning?.()
    }
    if (next === 0 && !expiredNotified) {
      expired.value = true
      expiredNotified = true
      options.onExpire?.()
      stop()
    }
  }

  function start(): void {
    stop()
    tick()
    if (!expired.value) {
      interval = setInterval(tick, 1000)
    }
  }

  function stop(): void {
    if (interval) {
      clearInterval(interval)
      interval = null
    }
  }

  const warning = computed(() => remaining.value > 0 && remaining.value <= warningSec)
  const elapsedRatio = computed(() => {
    if (durationSec === 0) return 1
    return Math.min(1, Math.max(0, 1 - remaining.value / durationSec))
  })

  if (getCurrentInstance()) {
    onUnmounted(stop)
  }

  return { remaining, warning, expired, elapsedRatio, start, stop }
}
