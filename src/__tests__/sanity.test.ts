import { describe, it, expect } from 'vitest'
import { ExpectationFailure, describeSelector } from '../core/index.js'

describe('sanity', () => {
  it('ExpectationFailure.toPrompt includes code and message', () => {
    const err = new ExpectationFailure({
      code: 'DRIVER_ERROR',
      message: 'test',
    })
    expect(err.toPrompt()).toContain('FAIL [DRIVER_ERROR]: test')
  })

  it('describeSelector renders a text selector', () => {
    expect(describeSelector({ text: 'Sign in' })).toBe('{ text="Sign in" }')
  })

  it('describeSelector renders a role+name selector', () => {
    expect(describeSelector({ role: 'button', name: 'Continue' }))
      .toBe('{ role=button, name="Continue" }')
  })
})
