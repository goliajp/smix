import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

import mdx from '@mdx-js/rollup'
import rehypeShiki from '@shikijs/rehype'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import rehypeAutolinkHeadings from 'rehype-autolink-headings'
import rehypeSlug from 'rehype-slug'
import remarkGfm from 'remark-gfm'
import { defineConfig } from 'vite'

const pkg = JSON.parse(readFileSync(resolve(import.meta.dirname, 'package.json'), 'utf-8'))

const deps = { ...pkg.dependencies, ...pkg.devDependencies }

export default defineConfig({
  base: '/',
  plugins: [
    tailwindcss(),
    mdx({
      remarkPlugins: [remarkGfm],
      rehypePlugins: [
        rehypeSlug,
        [
          rehypeShiki,
          {
            // Use warm-tinted themes that harmonize with the cream/dark
            // palette. defaultColor:false makes shiki emit BOTH theme
            // colors as CSS variables (--shiki-light, --shiki-dark) so
            // theme.css can swap them via `[data-theme='dark']`.
            themes: { light: 'vitesse-light', dark: 'vitesse-dark' },
            defaultColor: false,
          },
        ],
        [rehypeAutolinkHeadings, { behavior: 'append' }],
      ],
    }),
    react(),
  ],
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
    // MDX dynamic imports invoke the vitest Vite transform on first
    // import — on CI cold start, shiki language grammar loading
    // (typescript/bash/ts/yaml/json) pushes a single import past the
    // 5s default. 20s gives enough headroom without masking real
    // hangs (typical local cached run is < 500ms per file).
    testTimeout: 20_000,
  },
})
