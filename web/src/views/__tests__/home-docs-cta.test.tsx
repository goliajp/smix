import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'

import { HomeView } from '../home'

describe('web v0.2 C2 — home docs CTA', () => {
  it('renders at least one link pointing into /docs', () => {
    render(
      <MemoryRouter>
        <HomeView />
      </MemoryRouter>
    )
    // v1.2.4 redesign — primary CTA is "quick start →" linking to
    // /docs/quick-start; the doc index section also exposes many
    // /docs/<slug> entries. Test loosened to assert presence rather
    // than copy.
    const docsLinks = screen
      .getAllByRole('link')
      .filter((a) => (a.getAttribute('href') ?? '').includes('/docs'))
    expect(docsLinks.length).toBeGreaterThan(0)
  })
})
