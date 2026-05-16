import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'

import { HomeView } from '../home'

describe('web v0.2 C2 — home Read-the-docs CTA', () => {
  it('renders a Read the docs link pointing into /docs', () => {
    render(
      <MemoryRouter>
        <HomeView />
      </MemoryRouter>
    )
    const link = screen.getByText(/Read the docs/i).closest('a')
    expect(link).not.toBeNull()
    const href = link!.getAttribute('href')
    expect(href).toContain('/docs')
  })
})
