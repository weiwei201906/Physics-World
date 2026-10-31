import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import router from './router'
import { useAuthStore } from './stores/auth'
import './styles/main.css'

async function bootstrap(): Promise<void> {
  const app = createApp(App)
  const pinia = createPinia()
  app.use(pinia)

  // Silent restore: refresh access token from httpOnly cookie before mount.
  const auth = useAuthStore()
  await auth.restoreSession()

  app.use(router)
  app.mount('#app')
}

void bootstrap()
