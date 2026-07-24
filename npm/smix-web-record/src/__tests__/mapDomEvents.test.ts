import { describe, expect, it } from 'vitest'
import { mapDomEvents, type CapturedDomEvent } from '../mapDomEvents.js'

const click = (e: Partial<CapturedDomEvent> = {}): CapturedDomEvent => ({
  kind: 'click',
  timestampMs: 1,
  ...e,
})
const input = (e: Partial<CapturedDomEvent> = {}): CapturedDomEvent => ({
  kind: 'input',
  timestampMs: 1,
  ...e,
})

const one = (events: CapturedDomEvent[]) => {
  const r = mapDomEvents(events)
  expect(r.actions).toHaveLength(1)
  return JSON.parse(r.actions[0] as string)
}

describe('mapDomEvents (web capture leg)', () => {
  it('click with data-testid becomes tap by id', () => {
    const a = one([click({ testId: 'login-btn', timestampMs: 42 })])
    expect(a.kind).toBe('tap')
    expect(a.selector).toEqual({ id: 'login-btn' })
    expect(a.timestampMs).toBe(42)
  })

  it('click falls back to role, then text, then drops', () => {
    expect(one([click({ role: 'button' })]).selector).toEqual({ role: 'button' })
    expect(one([click({ text: 'Sign In' })]).selector).toEqual({ text: 'Sign In' })
    const r = mapDomEvents([click({})]) // no testid/role/text
    expect(r.actions).toHaveLength(0)
    expect(r.unmapped).toBe(1)
  })

  it('unmappable ARIA role (textbox) is a gap, not faked', () => {
    // textbox != smix textField; with no testid it drops.
    const r = mapDomEvents([click({ role: 'textbox' })])
    expect(r.actions).toHaveLength(0)
    expect(r.unmapped).toBe(1)
  })

  it('input becomes fill; consecutive inputs coalesce to the final value', () => {
    const a = one([
      input({ testId: 'q', value: 's', beforeValue: '' }),
      input({ testId: 'q', value: 'sm', beforeValue: 's' }),
      input({ testId: 'q', value: 'smix', beforeValue: 'sm' }),
    ])
    expect(a.kind).toBe('fill')
    expect(a.text).toBe('smix')
    expect(a.selector).toEqual({ id: 'q' })
  })

  it('input emptying a non-empty field becomes clear', () => {
    const a = one([input({ testId: 'q', value: '', beforeValue: 'smix' })])
    expect(a.kind).toBe('clear')
  })

  it('a click between input runs on the same field separates them', () => {
    const r = mapDomEvents([
      input({ testId: 'q', value: 'a', beforeValue: '' }),
      click({ testId: 'submit' }),
      input({ testId: 'q', value: 'b', beforeValue: '' }),
    ])
    expect(r.actions.map((s) => JSON.parse(s).kind)).toEqual(['fill', 'tap', 'fill'])
  })
})
