import { ref } from 'vue'

export type ToastTone = 'info' | 'success' | 'warning' | 'error'

export interface ToastItem {
  id: number
  message: string
  tone: ToastTone
}

const toasts = ref<ToastItem[]>([])
let sequence = 0
const timers = new Map<number, ReturnType<typeof setTimeout>>()

export function dismissToast(id: number): void {
  const timer = timers.get(id)
  if (timer) {
    clearTimeout(timer)
    timers.delete(id)
  }
  toasts.value = toasts.value.filter((item) => item.id !== id)
}

export function showToast(message: string, tone: ToastTone = 'info', durationMs = 5000): number {
  const id = (sequence += 1)
  toasts.value = [...toasts.value, { id, message, tone }]
  if (durationMs > 0) {
    timers.set(
      id,
      setTimeout(() => {
        dismissToast(id)
      }, durationMs)
    )
  }
  return id
}

export function clearToasts(): void {
  timers.forEach((timer) => clearTimeout(timer))
  timers.clear()
  toasts.value = []
}

export function useToast() {
  return { toasts, showToast, dismissToast, clearToasts }
}
