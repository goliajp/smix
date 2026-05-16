import { Navigate, type RouteObject } from 'react-router'

import { AppLayout } from '../../app'
import { HomeView } from '../home'
import { DocsLayout } from './layout'
import { DocsPage } from './page'

export const appRoutes: RouteObject[] = [
  {
    element: <AppLayout />,
    path: '/',
    children: [
      { index: true, element: <HomeView /> },
      {
        path: 'docs',
        element: <DocsLayout />,
        children: [
          { index: true, element: <Navigate replace to="/docs/quick-start" /> },
          { path: ':slug', element: <DocsPage /> },
        ],
      },
      { path: '*', element: <Navigate replace to="/" /> },
    ],
  },
]
