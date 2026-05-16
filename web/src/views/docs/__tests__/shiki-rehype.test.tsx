import { render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

describe('web v0.2 C3 — shiki build-time highlight', () => {
  it('quick-start.mdx renders fenced code with shiki classes + token styles', async () => {
    const mod = await import('../../../../content/quick-start.mdx')
    const Mdx = mod.default
    const { container } = render(<Mdx />)
    const pre = container.querySelector('pre.shiki')
    expect(pre).not.toBeNull()
    const styledSpan = container.querySelector('pre.shiki span[style*="color"]')
    expect(styledSpan).not.toBeNull()
  })
})
