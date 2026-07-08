// Thin client for the smix-server capture/observability API. The dev server
// proxies /api → http://127.0.0.1:8787 (see vite.config.ts), so these are
// same-origin relative fetches with no CORS dance.

import type { SimEntry } from '../views/live'

async function postCapture(action: 'start' | 'stop', udid: string): Promise<void> {
  const res = await fetch(`/api/capture/${action}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ udid }),
  })
  if (!res.ok) {
    throw new Error(`capture ${action} failed: ${res.status} ${res.statusText}`)
  }
}

export async function getSims(): Promise<SimEntry[]> {
  const res = await fetch('/api/sims')
  if (!res.ok) {
    throw new Error(`GET /api/sims failed: ${res.status} ${res.statusText}`)
  }
  return (await res.json()) as SimEntry[]
}

export async function startCapture(udid: string): Promise<void> {
  return postCapture('start', udid)
}

export async function stopCapture(udid: string): Promise<void> {
  return postCapture('stop', udid)
}
