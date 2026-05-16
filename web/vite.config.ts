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
        [rehypeShiki, { themes: { light: 'github-light', dark: 'github-dark' } }],
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
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.test.{ts,tsx}'],
    setupFiles: ['./src/test-setup.ts'],
  },
})
