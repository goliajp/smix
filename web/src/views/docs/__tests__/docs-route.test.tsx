import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { appRoutes } from '../routes'

describe('web v0.2 C1 — docs framework', () => {
  it('quick-start.mdx renders into the DOM', async () => {
    const mod = await import('../../../../content/quick-start.mdx')
    const Mdx = mod.default
    render(<Mdx />)
    // Real content lands in C2; just verify the mdx loader path renders *something*.
    expect(screen.getByRole('heading', { level: 1 })).toBeInTheDocument()
  })

  it('nav.config exports at least one item starting with quick-start', async () => {
    const mod = await import('../../../../content/nav.config')
    const nav = mod.default
    expect(Array.isArray(nav)).toBe(true)
    expect(nav.length).toBeGreaterThanOrEqual(1)
    expect(nav[0].slug).toBe('quick-start')
  })

  it('unknown docs slug renders docs-local Not found (not SPA root redirect)', async () => {
    const { createMemoryRouter, RouterProvider } = await import('react-router')
    const router = createMemoryRouter(appRoutes, { initialEntries: ['/docs/__nope__'] })
    render(<RouterProvider router={router} />)
    // wait for the lazy mdx loader / docs page render to settle
    await screen.findByText(/Not found/i)
    expect(screen.getByText(/Not found/i)).toBeInTheDocument()
  })
})
