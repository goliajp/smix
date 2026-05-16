import { describe, it, expect } from 'vitest'
import {
  compatibilitySchema,
  doctorCheckSchema,
  doctorReportSchema,
} from '../commands/doctor-schemas.js'

describe('doctor zod schemas (v0.7 C4)', () => {
  it('compatibilitySchema parses a valid round-trip', () => {
    const fixture = {
      status: 'supported' as const,
      required: 'Xcode >=26.0',
      actual: '26.4.1',
    }
    const parsed = compatibilitySchema.parse(fixture)
    expect(parsed).toEqual(fixture)
    expect(parsed.status).toBe('supported')
  })

  it('compatibilitySchema rejects invalid status enum', () => {
    const fixture = { status: 'BOGUS' }
    const result = compatibilitySchema.safeParse(fixture)
    expect(result.success).toBe(false)
    if (!result.success) {
      expect(
        result.error.issues.some((i) => i.path.includes('status')),
      ).toBe(true)
    }
  })

  it('doctorCheckSchema parses with optional compatibility (present and absent)', () => {
    const fixtureA = {
      name: 'bun' as const,
      ok: true,
      value: '1.3.14',
      compatibility: {
        status: 'supported' as const,
        required: '>=1.1.0',
        actual: '1.3.14',
      },
    }
    const parsedA = doctorCheckSchema.parse(fixtureA)
    expect(parsedA).toEqual(fixtureA)
    expect(parsedA.compatibility?.status).toBe('supported')

    const fixtureB = {
      name: 'xcode' as const,
      ok: true,
      value: 'Xcode 26.4.1',
    }
    const parsedB = doctorCheckSchema.parse(fixtureB)
    expect(parsedB).toEqual(fixtureB)
    expect(parsedB.compatibility).toBeUndefined()
  })

  it('doctorReportSchema parses a 6-check report round-trip', () => {
    const fixture = {
      exitCode: 0 as const,
      checks: [
        {
          name: 'xcode' as const,
          ok: true,
          value: 'Xcode 26.4.1',
          compatibility: {
            status: 'supported' as const,
            required: 'Xcode >=26.0',
            actual: '26.4.1',
          },
        },
        {
          name: 'runtimes' as const,
          ok: true,
          value: {
            count: 1,
            items: [
              {
                identifier: 'com.apple.CoreSimulator.SimRuntime.iOS-26-4',
                name: 'iOS 26.4',
                version: '26.4.1',
                isAvailable: true,
              },
            ],
          },
          compatibility: {
            status: 'supported' as const,
            required: 'iOS 26.x present (17.5 / 18.4 nice-to-have)',
            actual: '1 available: com.apple.CoreSimulator.SimRuntime.iOS-26-4',
          },
        },
        {
          name: 'claude' as const,
          ok: true,
          value: { path: '/usr/local/bin/claude', loggedIn: true },
          compatibility: {
            status: 'supported' as const,
            required: 'logged-in',
            actual: 'logged-in',
          },
        },
        {
          name: 'bun' as const,
          ok: true,
          value: '1.3.14',
          compatibility: {
            status: 'supported' as const,
            required: '>=1.1.0',
            actual: '1.3.14',
          },
        },
        {
          name: 'hid' as const,
          ok: true,
          value: { digitizer: true, indigo9: true },
          compatibility: {
            status: 'supported' as const,
            required: 'digitizer + indigo9',
            actual: 'digitizer=true, indigo9=true',
          },
        },
        {
          name: 'axp' as const,
          ok: true,
          value: {
            framework: { path: '/x', loaded: true },
            classes: { resolved: ['A'], missing: [] },
            selectors: { resolved: ['s'], missing: [] },
            sharedInstance: { available: true },
          },
          compatibility: {
            status: 'supported' as const,
            required: 'framework + 5 classes + 3 selectors + +sharedInstance',
            actual: 'framework=true, classes=1/1, selectors=1/1, sharedInstance=true',
          },
        },
      ],
    }
    const parsed = doctorReportSchema.parse(fixture)
    expect(parsed).toEqual(fixture)
    expect(parsed.checks).toHaveLength(6)
    expect(parsed.checks[0]?.compatibility?.status).toBe('supported')
  })

  it('doctorReportSchema rejects bad check name enum', () => {
    const fixture = {
      exitCode: 0,
      checks: [{ name: 'BOGUS', ok: true, value: '' }],
    }
    const result = doctorReportSchema.safeParse(fixture)
    expect(result.success).toBe(false)
    if (!result.success) {
      expect(
        result.error.issues.some((i) => i.path.includes('name')),
      ).toBe(true)
    }
  })
})
