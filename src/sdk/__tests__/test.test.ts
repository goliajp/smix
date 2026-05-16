import { describe, it, expect, beforeEach, vi } from 'vitest'
import { test, run, resetRegistry } from '../test.js'

beforeEach(() => {
  resetRegistry()
})

describe('C5: bail option (v0.5 C5)', () => {
  it('bail: true + first case fails → second case not invoked', async () => {
    test('a', () => {})
    test('b', () => {
      throw new Error('boom')
    })
    test('c', () => {})
    const onStart = vi.fn()
    const res = await run({ bail: true, onCaseStart: onStart })
    expect(res.passed).toBe(1)
    expect(res.failed).toBe(1)
    expect(res.failures).toHaveLength(1)
    expect(res.failures[0]?.name).toBe('b')
    // 'a' + 'b' invoked; 'c' must not be invoked
    const invokedNames = onStart.mock.calls.map((c) => c[0])
    expect(invokedNames).toEqual(['a', 'b'])
  })

  it('bail: false (default) → all cases run despite earlier fail', async () => {
    test('a', () => {})
    test('b', () => {
      throw new Error('boom')
    })
    test('c', () => {})
    const onStart = vi.fn()
    const res = await run({ onCaseStart: onStart })
    expect(res.passed).toBe(2)
    expect(res.failed).toBe(1)
    const invokedNames = onStart.mock.calls.map((c) => c[0])
    expect(invokedNames).toEqual(['a', 'b', 'c'])
  })

  it('bail: true + 0 failure → all cases run (no-op)', async () => {
    test('a', () => {})
    test('b', () => {})
    const onStart = vi.fn()
    const res = await run({ bail: true, onCaseStart: onStart })
    expect(res.passed).toBe(2)
    expect(res.failed).toBe(0)
    expect(onStart.mock.calls.map((c) => c[0])).toEqual(['a', 'b'])
  })

  it('bail + grep combined → grep filter first, bail on first fail in subset', async () => {
    test('a', () => {})
    test('b-fail', () => {
      throw new Error('x')
    })
    test('c-fail', () => {
      throw new Error('y')
    })
    test('d', () => {})
    const onStart = vi.fn()
    const res = await run({ grep: 'fail', bail: true, onCaseStart: onStart })
    expect(res.passed).toBe(0)
    expect(res.failed).toBe(1)
    expect(res.failures[0]?.name).toBe('b-fail')
    // Only 'b-fail' invoked from the 2-element filtered subset
    const invokedNames = onStart.mock.calls.map((c) => c[0])
    expect(invokedNames).toEqual(['b-fail'])
  })
})
