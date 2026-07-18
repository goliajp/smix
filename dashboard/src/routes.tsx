import { Navigate, type RouteObject } from 'react-router'

import { AppLayout } from './app'
import { HomeView } from './views/home'
import { LiveView } from './views/live'

export const appRoutes: RouteObject[] = [
  {
    element: <AppLayout />,
    path: '/',
    children: [
      { index: true, element: <HomeView /> },
      { path: 'live', element: <LiveView /> },
      { path: '*', element: <Navigate replace to="/" /> },
    ],
  },
]
