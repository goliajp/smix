import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'

import { HomeView } from '../home'

describe('home — minimal dashboard landing', () => {
  it('links to the /live observation panel and the GitHub repo', () => {
    render(
      <MemoryRouter>
        <HomeView />
      </MemoryRouter>
    )
    const liveLink = screen.getByRole('link', { name: /open live panel/i })
    expect(liveLink.getAttribute('href')).toBe('/live')
    const ghLink = screen.getByRole('link', { name: /github/i })
    expect(ghLink.getAttribute('href')).toBe('https://github.com/goliajp/smix')
  })

  it('states the one-line description and carries no marketing claims', () => {
    render(
      <MemoryRouter>
        <HomeView />
      </MemoryRouter>
    )
    expect(
      screen.getByRole('heading', {
        name: /AI-native UI automation for the iOS Simulator and Android emulator/i,
      })
    ).toBeInTheDocument()
    expect(screen.queryByText(/6\.73/)).toBeNull()
    expect(screen.queryByText(/maestro/i)).toBeNull()
    expect(screen.queryByText(/27/)).toBeNull()
    expect(document.querySelector('a[href^="/docs"]')).toBeNull()
  })
})
