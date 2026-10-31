import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath, URL } from 'node:url'

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url))
    }
  },
  server: {
    port: 5173,
    host: true,
    proxy: {
      '/auth': { target: 'http://127.0.0.1:8080', changeOrigin: true },
      '/.well-known': { target: 'http://127.0.0.1:8080', changeOrigin: true },
      '/graphql': { target: 'http://127.0.0.1:8080', changeOrigin: true },
      '/graphiql': { target: 'http://127.0.0.1:8080', changeOrigin: true },
      '/config': { target: 'http://127.0.0.1:8080', changeOrigin: true },
      '/ws': { target: 'ws://127.0.0.1:8080', ws: true }
    }
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./tests/setup.ts'],
    include: ['tests/**/*.{test,spec}.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html'],
      include: ['src/**/*.ts', 'src/**/*.vue']
    }
  }
})
