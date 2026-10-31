import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import router from './router'
import { useAuthStore } from './stores/auth'
import { useConfigStore } from './stores/config'
import './styles/main.css'

async function bootstrap(): Promise<void> {
  const app = createApp(App)
  const pinia = createPinia()
  app.use(pinia)

  // Runtime config is fetched first so auth timing follows server values.
  const config = useConfigStore()
  await config.init()

  // Silent restore: refresh access token from the httpOnly cookie before mount.
  const auth = useAuthStore()
  await auth.restoreSession()

  app.use(router)
  app.mount('#app')
}

void bootstrap()
