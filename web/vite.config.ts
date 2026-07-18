import mdx from '@mdx-js/rollup'
import rehypeShiki from '@shikijs/rehype'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import remarkGfm from 'remark-gfm'
import { defineConfig } from 'vite'

// Marketing site is a single static page at the domain root.
export default defineConfig({
  base: '/',
  plugins: [
    tailwindcss(),
    mdx({
      remarkPlugins: [remarkGfm],
      rehypePlugins: [
        [
          rehypeShiki,
          {
            // defaultColor:false emits BOTH theme colors as CSS vars
            // (--shiki-light / --shiki-dark) so theme.css swaps them by
            // [data-theme]. Same bridge the dashboard uses.
            themes: { light: 'github-light', dark: 'github-dark' },
            defaultColor: false,
          },
        ],
      ],
    }),
    react(),
  ],
})
