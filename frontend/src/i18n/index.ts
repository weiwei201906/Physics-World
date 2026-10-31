import { ref } from 'vue'
import zhCN from '@/locales/zh-CN.json'
import enUS from '@/locales/en-US.json'

export type Locale = 'zh-CN' | 'en-US'

export const availableLocales: Locale[] = ['zh-CN', 'en-US']

const LOCALE_STORAGE_KEY = 'pw-locale'

const messages: Record<Locale, typeof zhCN> = {
  'zh-CN': zhCN,
  'en-US': enUS as typeof zhCN
}

function detectInitialLocale(): Locale {
  if (typeof localStorage !== 'undefined') {
    const saved = localStorage.getItem(LOCALE_STORAGE_KEY)
    if (saved === 'zh-CN' || saved === 'en-US') return saved
  }
  if (typeof navigator !== 'undefined' && navigator.language) {
    return navigator.language.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en-US'
  }
  return 'zh-CN'
}

export const locale = ref<Locale>(detectInitialLocale())

export function setLocale(next: Locale): void {
  if (next === locale.value) return
  locale.value = next
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(LOCALE_STORAGE_KEY, next)
  }
}

type Primitive = string | number | boolean | null | undefined

type Leaves<T, Prefix extends string = ''> = T extends Primitive
  ? never
  : {
      [K in keyof T & string]: T[K] extends Primitive
        ? `${Prefix}${K}`
        : Leaves<T[K], `${Prefix}${K}.`>
    }[keyof T & string]

export type TranslationKey = Leaves<typeof zhCN>

export type TranslationParams = Record<string, string | number>

function readPath(key: string): unknown {
  const parts = key.split('.')
  let cursor: unknown = messages[locale.value]
  for (const part of parts) {
    if (typeof cursor !== 'object' || cursor === null) return undefined
    cursor = (cursor as Record<string, unknown>)[part]
  }
  return cursor
}

export function hasKey(key: string): boolean {
  return typeof readPath(key) === 'string'
}

function interpolate(template: string, params?: TranslationParams): string {
  if (!params) return template
  return template.replace(/\{(\w+)\}/g, (match, name: string) => {
    const value = params[name]
    return value === undefined ? match : String(value)
  })
}

/**
 * Translate a nested locale key. Unknown keys fall back to the key itself so a
 * missing translation is visible during development and never renders blank UI.
 * Reads the active `locale` ref so callers re-render when the language changes.
 */
export function t(key: TranslationKey, params?: TranslationParams): string {
  const template = readPath(key)
  if (typeof template !== 'string') return key
  return interpolate(template, params)
}

/**
 * Format a ratio (0..1) as a whole percentage without relying on locale APIs.
 */
export function percent(value: number): string {
  if (!Number.isFinite(value)) return '0'
  return String(Math.round(Math.max(0, Math.min(1, value)) * 100))
}
