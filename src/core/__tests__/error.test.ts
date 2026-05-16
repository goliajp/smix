import { describe, it, expect } from 'vitest'
import { ExpectationFailure } from '../error.js'

describe('ExpectationFailure.toPrompt suggestions rendering (v0.4 C3)', () => {
  it('renders suggestions block after selector, before visible elements', () => {
    const f = new ExpectationFailure({
      code: 'NOT_VISIBLE',
      message: 'demo',
      selector: { text: 'Genral' },
      suggestions: ['Did you mean "General"? (similarity 0.86, field name)'],
      visibleElements: [
        {
          role: 'cell',
          name: 'General',
          bounds: { x: 0, y: 0, w: 10, h: 10 },
          enabled: true,
        },
      ],
    })
    const p = f.toPrompt()
    const selIdx = p.indexOf('selector:')
    const sugIdx = p.indexOf('suggestions:')
    const visIdx = p.indexOf('visible elements')
    expect(selIdx).toBeGreaterThanOrEqual(0)
    expect(sugIdx).toBeGreaterThan(selIdx)
    expect(visIdx).toBeGreaterThan(sugIdx)
    expect(p).toContain('    - Did you mean "General"? (similarity 0.86, field name)')
  })

  it('omits suggestions block when suggestions is empty', () => {
    const f = new ExpectationFailure({
      code: 'NOT_VISIBLE',
      message: 'demo',
      suggestions: [],
    })
    expect(f.toPrompt()).not.toContain('suggestions:')
  })
})
