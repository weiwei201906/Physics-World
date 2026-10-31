/// <reference types="vite/client" />

declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>
  export default component
}

interface ImportMetaEnv {
  readonly VITE_API_BASE_URL: string
  readonly VITE_GRAPHQL_ENDPOINT: string
  readonly VITE_REC_GRAPHQL_ENDPOINT: string
  readonly VITE_BATTLE_WS_URL: string
  readonly VITE_SSE_EVENTS_URL: string
  readonly VITE_CDN_MEDIA_BASE_URL: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
