import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    include: ['src/**/*.test.ts'],
    exclude: ['**/node_modules/**', 'dist/**', '**/integration/**'],
    reporters: ['default'],
    setupFiles: ['src/test-setup.ts'],
  },
})
