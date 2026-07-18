import { render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

describe('web v0.2 C3 — examples index + 3 sub-pages', () => {
  it('examples.mdx index links to all 3 sub-pages', async () => {
    const mod = await import('../../../../content/examples.mdx')
    const Mdx = mod.default
    const { container } = render(<Mdx />)
    const hrefs = Array.from(container.querySelectorAll('a[href]')).map((a) =>
      a.getAttribute('href')
    )
    expect(hrefs).toEqual(
      expect.arrayContaining([
        '/docs/examples-login-tap',
        '/docs/examples-tap-text-selector',
        '/docs/examples-screenshot-only',
      ])
    )
  })

  it('examples-login-tap.mdx embeds the full source verbatim', async () => {
    const mod = await import('../../../../content/examples-login-tap.mdx')
    const Mdx = mod.default
    const { container } = render(<Mdx />)
    expect(container.textContent).toContain("app.tap({ text: 'General' })")
  })

  it('examples-screenshot-only.mdx embeds the full source verbatim', async () => {
    const mod = await import('../../../../content/examples-screenshot-only.mdx')
    const Mdx = mod.default
    const { container } = render(<Mdx />)
    expect(container.textContent).toContain('app.screenshot()')
    expect(container.textContent).toContain('com.apple.mobilesafari')
  })
})
