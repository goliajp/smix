import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

const pkg = JSON.parse(readFileSync(resolve(import.meta.dirname, 'package.json'), 'utf-8'))

const deps = { ...pkg.dependencies, ...pkg.devDependencies }

export default defineConfig({
  base: '/',
  plugins: [tailwindcss(), react()],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
    __DEP_VERSIONS__: JSON.stringify(deps),
  },
  resolve: {
    alias: { '@': resolve(import.meta.dirname, 'src') },
  },
  server: {
    proxy: {
      '/api': { target: 'http://127.0.0.1:8787', changeOrigin: true },
      '/streams': { target: 'http://127.0.0.1:8787', changeOrigin: true },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.test.{ts,tsx}'],
    setupFiles: ['./src/test-setup.ts'],
  },
})
