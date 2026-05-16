import { describe, it, expect } from 'vitest'
import { elementTypeNameToRole, elementTypeToRole } from '../role.js'

describe('elementTypeNameToRole', () => {
  it('button maps to "button"', () => {
    expect(elementTypeNameToRole('button')).toBe('button')
  })

  it('staticText maps to "staticText"', () => {
    expect(elementTypeNameToRole('staticText')).toBe('staticText')
  })

  it('cell maps to "cell"', () => {
    expect(elementTypeNameToRole('cell')).toBe('cell')
  })

  it('"other" — in XCUIElementType table but not KNOWN_ROLES → undefined', () => {
    expect(elementTypeNameToRole('other')).toBeUndefined()
  })

  it('"unknownXyz" — not in XCUIElementType table → undefined', () => {
    expect(elementTypeNameToRole('unknownXyz')).toBeUndefined()
  })

  it('"group" — in table (id 3) but not KNOWN_ROLES → undefined', () => {
    expect(elementTypeNameToRole('group')).toBeUndefined()
  })

  it('switch maps to "switch"', () => {
    expect(elementTypeNameToRole('switch')).toBe('switch')
  })

  it('toggle maps to "toggle"', () => {
    expect(elementTypeNameToRole('toggle')).toBe('toggle')
  })

  it('empty string → undefined (edge case)', () => {
    expect(elementTypeNameToRole('')).toBeUndefined()
  })

  it('navigationBar maps to "navigationBar"', () => {
    expect(elementTypeNameToRole('navigationBar')).toBe('navigationBar')
  })

  it('"application" — in table (id 2) but not KNOWN_ROLES → undefined (root app node)', () => {
    expect(elementTypeNameToRole('application')).toBeUndefined()
  })

  it('result agrees with numeric variant for every KNOWN_ROLES entry', () => {
    // Defensive: for each KNOWN_ROLES name, the string variant must match the
    // numeric variant when fed the same XCUIElementType id. Guards against
    // future drift between elementTypeToRole(number) and elementTypeNameToRole(string).
    const names: string[] = [
      'button', 'cell', 'staticText', 'switch', 'toggle', 'navigationBar',
      'link', 'image', 'slider', 'webView',
    ]
    for (const n of names) {
      const fromName = elementTypeNameToRole(n)
      // looking up the numeric id and calling the legacy resolver
      const idTable: Record<string, number> = {
        button: 9, cell: 75, staticText: 48, switch: 40, toggle: 41,
        navigationBar: 21, link: 42, image: 43, slider: 33, webView: 58,
      }
      const id = idTable[n]!
      expect(fromName).toBe(elementTypeToRole(id))
    }
  })
})
