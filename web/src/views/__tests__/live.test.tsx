import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { createMemoryRouter, MemoryRouter, RouterProvider } from 'react-router'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { appRoutes } from '../docs/routes'
import { LiveView, type SimEntry } from '../live'

const ONE_SIM: SimEntry[] = [
  {
    udid: 'TEST-UDID',
    device_name: 'iPhone 16 Pro',
    runtime: 'iOS 26.5',
    capturing: true,
    started_at: '2026-05-30T00:00:00Z',
    stream_path: '/streams/TEST-UDID/index.m3u8',
  },
]

const FOUR_SIMS: SimEntry[] = [
  {
    udid: 'UDID-A',
    device_name: 'iPhone 16',
    runtime: 'iOS 26.5',
    capturing: true,
    started_at: '2026-05-30T00:00:00Z',
    stream_path: '/streams/UDID-A/index.m3u8',
  },
  {
    udid: 'UDID-B',
    device_name: 'iPhone 16 Pro',
    runtime: 'iOS 26.5',
    capturing: true,
    started_at: '2026-05-30T00:00:00Z',
    stream_path: '/streams/UDID-B/index.m3u8',
  },
  {
    udid: 'UDID-C',
    device_name: 'iPhone 17',
    runtime: 'iOS 26.5',
    capturing: false,
    started_at: '2026-05-30T00:00:00Z',
    stream_path: '/streams/UDID-C/index.m3u8',
  },
  {
    udid: 'UDID-D',
    device_name: 'iPhone 17 Pro',
    runtime: 'iOS 26.5',
    capturing: true,
    started_at: '2026-05-30T00:00:00Z',
    stream_path: '/streams/UDID-D/index.m3u8',
  },
]

describe('web v0.3 C4 — /live viewer', () => {
  it('renders sim metadata, capture controls, and a video container', () => {
    const { container } = render(
      <MemoryRouter>
        <LiveView sims={ONE_SIM} />
      </MemoryRouter>
    )
    // metadata surfaced (udid appears in rail + sidebar; device + runtime in sidebar)
    expect(screen.getAllByText(/TEST-UDID/).length).toBeGreaterThan(0)
    // device_name shows in both the rail and the sidebar (by design)
    expect(screen.getAllByText('iPhone 16 Pro').length).toBeGreaterThan(0)
    expect(screen.getByText('iOS 26.5')).toBeInTheDocument()
    // capture controls: both START and STOP present (enabled by capturing state)
    expect(screen.getByRole('button', { name: /start/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /^stop$/i })).toBeInTheDocument()
    // video container present (hls.js attaches here in S2)
    expect(container.querySelector('video')).not.toBeNull()
  })

  it('shows a friendly empty state when there are no sims', () => {
    render(
      <MemoryRouter>
        <LiveView sims={[]} />
      </MemoryRouter>
    )
    expect(screen.getByText(/no sims/i)).toBeInTheDocument()
  })

  it('live_route_registered: /live resolves to LiveView, not the * redirect', async () => {
    // No injected sims and fetch unmocked here, so guard the network call.
    vi.stubGlobal(
      'fetch',
      vi.fn(() => Promise.resolve({ ok: true, json: () => Promise.resolve([]) }))
    )
    const router = createMemoryRouter(appRoutes, { initialEntries: ['/live'] })
    render(<RouterProvider router={router} />)
    // LiveView's left rail label "sims" is its distinctive marker (the * route
    // would redirect to "/" → HomeView, which has no such rail).
    await waitFor(() => {
      expect(screen.getByText(/^sims$/i)).toBeInTheDocument()
    })
  })

  it('renders grid mode with N cells (default when N>=2 sims injected)', () => {
    const { container } = render(
      <MemoryRouter>
        <LiveView sims={FOUR_SIMS} />
      </MemoryRouter>
    )
    const videos = container.querySelectorAll('video')
    expect(videos.length).toBe(4)
    const udids = Array.from(videos).map((v) => v.getAttribute('data-udid'))
    expect(udids.sort()).toEqual(['UDID-A', 'UDID-B', 'UDID-C', 'UDID-D'])
  })

  it('grid cell click switches to single-sim detail mode (showing one <video>)', async () => {
    const { container, findAllByRole } = render(
      <MemoryRouter>
        <LiveView sims={FOUR_SIMS} />
      </MemoryRouter>
    )
    // grid 4 videos initially
    expect(container.querySelectorAll('video').length).toBe(4)

    // click the cell for UDID-B — we use role=button with name containing the udid head
    const cellButtons = await findAllByRole('button', { name: /UDID-B/i })
    const cellButton = cellButtons.find(
      (b) => b.getAttribute('data-grid-cell') === 'true' || b.dataset.gridCell === 'true'
    )
    expect(cellButton, 'a grid cell button for UDID-B must exist').toBeDefined()
    fireEvent.click(cellButton!)

    const videosAfter = container.querySelectorAll('video')
    expect(videosAfter.length).toBe(1)
    expect(videosAfter[0]!.getAttribute('data-udid')).toBe('UDID-B')
  })

  it('N=1 falls back to single mode (1 video, not grid)', () => {
    const { container } = render(
      <MemoryRouter>
        <LiveView sims={ONE_SIM} />
      </MemoryRouter>
    )
    expect(container.querySelectorAll('video').length).toBe(1)
  })

  it('fetches_sims_from_api: renders sims fetched from GET /api/sims', async () => {
    const fetched: SimEntry[] = [
      {
        udid: 'FETCHED-UDID-0001',
        device_name: 'iPhone 16',
        runtime: 'iOS 26.5',
        capturing: false,
        started_at: '2026-05-30T00:00:00Z',
        stream_path: '/streams/FETCHED-UDID-0001/index.m3u8',
      },
    ]
    const fetchMock = vi.fn((url: string) => {
      if (url === '/api/sims') {
        return Promise.resolve({ ok: true, json: () => Promise.resolve(fetched) })
      }
      return Promise.reject(new Error(`unexpected fetch: ${url}`))
    })
    vi.stubGlobal('fetch', fetchMock)

    render(
      <MemoryRouter>
        <LiveView />
      </MemoryRouter>
    )
    await waitFor(() => {
      expect(screen.getAllByText(/FETCHED-UDID-0001/).length).toBeGreaterThan(0)
    })
    expect(fetchMock).toHaveBeenCalledWith('/api/sims')
  })
})

afterEach(() => {
  vi.unstubAllGlobals()
})
