import './index.css'

import { loadPersistedTheme, resolveThemeCssVars } from '@goliapkg/gds/systems'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { createBrowserRouter, RouterProvider } from 'react-router'

import { appRoutes } from './views/docs/routes'

// pre-render theme to avoid FOUC
const saved = loadPersistedTheme()
if (saved) {
  const mode =
    saved.mode === 'system'
      ? window.matchMedia('(prefers-color-scheme: dark)').matches
        ? 'dark'
        : 'light'
      : saved.mode
  const vars = resolveThemeCssVars(saved, mode as 'dark' | 'light')
  const root = document.documentElement
  for (const [k, v] of Object.entries(vars)) root.style.setProperty(k, v as string)
  root.dataset.theme = mode
}

const router = createBrowserRouter(appRoutes)

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>
)
