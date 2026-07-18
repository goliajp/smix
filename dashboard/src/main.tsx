import './index.css'

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { createBrowserRouter, RouterProvider } from 'react-router'

import { preMountApplyTheme } from './components/theme-bootstrap'
import { appRoutes } from './routes'

// Pre-paint persisted theme before React mounts to avoid FOUC.
preMountApplyTheme()

const router = createBrowserRouter(appRoutes)

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>
)
