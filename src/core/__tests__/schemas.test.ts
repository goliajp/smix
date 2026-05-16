import { describe, it, expect } from 'vitest'
import {
  screenDescriptionSchema,
  serializedFailureSchema,
  a11yNodeSchema,
} from '../schemas.js'

describe('core zod schemas (v0.6 C7)', () => {
  it('screenDescriptionSchema parses a valid fixture round-trip', () => {
    const fixture = {
      screenshot: 'YmFzZTY0LWJ5dGVz',
      elements: [
        {
          role: 'button' as const,
          bounds: { x: 0, y: 0, w: 100, h: 44 },
          enabled: true,
          name: 'Login',
        },
      ],
      frontApp: 'com.acme.app',
      summary: 'Login screen visible',
      capturedAt: 1700000000000,
    }
    const parsed = screenDescriptionSchema.parse(fixture)
    expect(parsed).toEqual(fixture)
    expect(parsed.elements[0]?.role).toBe('button')
    expect(parsed.elements[0]?.bounds.w).toBe(100)
  })

  it('screenDescriptionSchema rejects invalid fixture (missing required fields)', () => {
    const fixture = { screenshot: 'x', elements: [] }
    const result = screenDescriptionSchema.safeParse(fixture)
    expect(result.success).toBe(false)
    if (!result.success) {
      expect(result.error.issues.length).toBeGreaterThanOrEqual(3)
    }
  })

  it('serializedFailureSchema parses a valid fixture round-trip', () => {
    const fixture = {
      ok: false as const,
      code: 'ELEMENT_NOT_FOUND' as const,
      message: 'No element matches',
      suggestions: ['try aria-label'],
      visibleElements: [
        {
          role: 'button' as const,
          bounds: { x: 0, y: 0, w: 10, h: 10 },
          enabled: true,
        },
      ],
    }
    const parsed = serializedFailureSchema.parse(fixture)
    expect(parsed.code).toBe('ELEMENT_NOT_FOUND')
    expect(parsed.suggestions[0]).toBe('try aria-label')
  })

  it('serializedFailureSchema rejects invalid fixture (bad code enum)', () => {
    const fixture = {
      ok: false,
      code: 'BOGUS_CODE',
      message: 'x',
      suggestions: [],
      visibleElements: [],
    }
    const result = serializedFailureSchema.safeParse(fixture)
    expect(result.success).toBe(false)
    if (!result.success) {
      expect(
        result.error.issues.some((i) => i.path.includes('code')),
      ).toBe(true)
    }
  })

  it('a11yNodeSchema parses recursive children via z.lazy', () => {
    const fixture = {
      rawType: 'window',
      bounds: { x: 0, y: 0, w: 390, h: 844 },
      enabled: true,
      selected: false,
      hasFocus: false,
      visible: true,
      children: [
        {
          rawType: 'button',
          label: 'OK',
          bounds: { x: 0, y: 0, w: 50, h: 30 },
          enabled: true,
          selected: false,
          hasFocus: false,
          visible: true,
          children: [],
        },
      ],
    }
    const parsed = a11yNodeSchema.parse(fixture) as {
      children: Array<{ label: string; children: unknown[] }>
    }
    expect(parsed.children[0]?.label).toBe('OK')
    expect(parsed.children[0]?.children.length).toBe(0)
  })
})
