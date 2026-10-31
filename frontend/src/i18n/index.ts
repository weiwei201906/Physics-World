import zhCN from '@/locales/zh-CN.json'

export const messages = zhCN

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
  let cursor: unknown = messages
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
