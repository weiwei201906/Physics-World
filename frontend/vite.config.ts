import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { cpSync, existsSync, mkdirSync } from 'node:fs'
import { fileURLToPath, URL } from 'node:url'

// Question diagrams live with the question bundle, outside the frontend tree.
// `vite build` clears dist, so copy them back on every build to keep /media
// URLs working without a separate deploy step.
function copyQbankMedia() {
  return {
    name: 'copy-qbank-media',
    apply: 'build' as const,
    closeBundle() {
      const source = fileURLToPath(new URL('../deploy/qbank/media', import.meta.url))
      if (!existsSync(source)) return
      const target = fileURLToPath(new URL('./dist/media', import.meta.url))
      mkdirSync(target, { recursive: true })
      cpSync(source, target, { recursive: true })
    }
  }
}

export default defineConfig({
  plugins: [vue(), copyQbankMedia()],
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
      '/events': { target: 'http://127.0.0.1:8080', changeOrigin: true },
      '/rec': { target: 'http://127.0.0.1:8080', changeOrigin: true },
      '/media': { target: 'http://127.0.0.1:8080', changeOrigin: true },
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
