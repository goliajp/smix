import { describe, it, expect, vi } from 'vitest'
import {
  runDoctorCommand,
  type AxpCapability,
  type ClaudeValue,
  type RunDoctorDeps,
  type RuntimesValue,
} from '../commands/doctor.js'

function makeBufs() {
  let out = ''
  let err = ''
  return {
    out: { write: (s: string) => { out += s } },
    err: { write: (s: string) => { err += s } },
    readOut: () => out,
    readErr: () => err,
  }
}

const AXP_FRAMEWORK_PATH =
  '/System/Library/PrivateFrameworks/AccessibilityPlatformTranslation.framework/AccessibilityPlatformTranslation'

const AXP_CLASS_NAMES: readonly string[] = [
  'AXPTranslator',
  'AXPMacPlatformElement',
  'AXPTranslationObject',
  'AXPTranslatorRequest',
  'AXPTranslatorResponse',
]
const AXP_SELECTOR_NAMES: readonly string[] = [
  'frontmostApplicationWithDisplayId:bridgeDelegateToken:',
  'macPlatformElementFromTranslation:',
  'objectAtPoint:displayId:bridgeDelegateToken:',
]

function okAxp(): AxpCapability {
  return {
    framework: { path: AXP_FRAMEWORK_PATH, loaded: true },
    classes: { resolved: [...AXP_CLASS_NAMES], missing: [] },
    selectors: { resolved: [...AXP_SELECTOR_NAMES], missing: [] },
    sharedInstance: { available: true },
  }
}

const RUNTIME_IOS_26: RuntimesValue['items'][number] = {
  identifier: 'com.apple.CoreSimulator.SimRuntime.iOS-26-4',
  name: 'iOS 26.4',
  version: '26.4.1',
  isAvailable: true,
}
const RUNTIME_IOS_18: RuntimesValue['items'][number] = {
  identifier: 'com.apple.CoreSimulator.SimRuntime.iOS-18-5',
  name: 'iOS 18.5',
  version: '18.5',
  isAvailable: true,
}
const RUNTIME_IOS_17: RuntimesValue['items'][number] = {
  identifier: 'com.apple.CoreSimulator.SimRuntime.iOS-17-5',
  name: 'iOS 17.5',
  version: '17.5',
  isAvailable: true,
}

function okRuntimes(): RuntimesValue {
  const items = [RUNTIME_IOS_26, RUNTIME_IOS_18, RUNTIME_IOS_17]
  return { count: items.length, items }
}

function okClaude(): ClaudeValue {
  return { path: '/usr/local/bin/claude', loggedIn: true }
}

function okDeps(overrides: Partial<RunDoctorDeps> = {}, bufs = makeBufs()): {
  deps: RunDoctorDeps
  bufs: ReturnType<typeof makeBufs>
} {
  return {
    deps: {
      probeXcode: vi.fn().mockResolvedValue('Xcode 26.4.1'),
      probeRuntimes: vi.fn().mockResolvedValue(okRuntimes()),
      probeClaude: vi.fn().mockResolvedValue(okClaude()),
      probeBun: vi.fn().mockReturnValue('1.3.13'),
      probeHidChannels: vi
        .fn()
        .mockResolvedValue({ digitizer: true, indigo9: true }),
      probeAxp: vi.fn().mockResolvedValue(okAxp()),
      json: false,
      out: bufs.out,
      err: bufs.err,
      ...overrides,
    },
    bufs,
  }
}

describe('runDoctorCommand', () => {
  it('happy: all 6 probes OK → exitCode 0, 6 lines with ✓', async () => {
    const { deps, bufs } = okDeps()
    const res = await runDoctorCommand(deps)
    expect(res).toEqual({ exitCode: 0 })
    const lines = bufs.readOut().split('\n').filter(Boolean)
    expect(lines).toHaveLength(6)
    expect(lines[0]).toMatch(/^✓ xcode: .*Xcode 26\.4\.1/)
    expect(lines[1]).toMatch(/^✓ runtimes: .*3/)
    expect(lines[2]).toMatch(/^✓ claude: .*claude/)
    expect(lines[3]).toMatch(/^✓ bun: .*1\.3\.13/)
    expect(lines[4]).toMatch(/^✓ hid: digitizer ✓, indigo9 ✓$/)
    expect(lines[5]).toMatch(
      /^✓ axp: framework ✓, classes 5\/5, selectors 3\/3, \+sharedInstance ✓$/,
    )
    expect(bufs.readErr()).toBe('')
  })

  it('--json: single line JSON with exitCode + 6 checks in order', async () => {
    const { deps, bufs } = okDeps({ json: true })
    const res = await runDoctorCommand(deps)
    expect(res).toEqual({ exitCode: 0 })
    const out = bufs.readOut().trim()
    const parsed = JSON.parse(out)
    expect(parsed.exitCode).toBe(0)
    expect(parsed.checks).toHaveLength(6)
    expect(parsed.checks.map((c: { name: string }) => c.name)).toEqual([
      'xcode',
      'runtimes',
      'claude',
      'bun',
      'hid',
      'axp',
    ])
    for (const c of parsed.checks) {
      expect(c.ok).toBe(true)
    }
    expect(parsed.checks[4].value).toEqual({ digitizer: true, indigo9: true })
    expect(parsed.checks[5].value).toEqual(okAxp())
    expect(bufs.readErr()).toBe('')
  })

  it('xcode probe reject → ok=false, exitCode=1, others still probed', async () => {
    const { deps, bufs } = okDeps({
      probeXcode: vi.fn().mockRejectedValueOnce(new Error('xcodebuild: command not found')),
    })
    const res = await runDoctorCommand(deps)
    expect(res.exitCode).toBe(1)
    const lines = bufs.readOut().split('\n').filter(Boolean)
    expect(lines[0]).toMatch(/^✗ xcode:.*xcodebuild/)
    expect(lines[1]).toMatch(/^✓ runtimes:/)
    expect(lines[2]).toMatch(/^✓ claude:/)
    expect(lines[3]).toMatch(/^✓ bun:/)
    expect(lines[4]).toMatch(/^✓ hid: digitizer ✓, indigo9 ✓$/)
    expect(lines[5]).toMatch(/^✓ axp: framework ✓/)
  })

  it('xcode unexpected output → ok=false', async () => {
    const { deps, bufs } = okDeps({
      probeXcode: vi.fn().mockResolvedValueOnce('GCC 4.2'),
    })
    const res = await runDoctorCommand(deps)
    expect(res.exitCode).toBe(1)
    expect(bufs.readOut()).toMatch(/✗ xcode:.*unexpected output.*GCC 4\.2/)
  })

  it('runtimes 0 → ok=false', async () => {
    const { deps, bufs } = okDeps({
      probeRuntimes: vi.fn().mockResolvedValueOnce({ count: 0, items: [] }),
    })
    const res = await runDoctorCommand(deps)
    expect(res.exitCode).toBe(1)
    expect(bufs.readOut()).toMatch(/✗ runtimes:.*no iOS runtime/)
  })

  it('claude not in PATH (probe returns path=undefined) → ok=false', async () => {
    const { deps, bufs } = okDeps({
      probeClaude: vi
        .fn()
        .mockResolvedValueOnce({ path: undefined, loggedIn: false }),
    })
    const res = await runDoctorCommand(deps)
    expect(res.exitCode).toBe(1)
    expect(bufs.readOut()).toMatch(/✗ claude:.*not found in PATH/)
  })

  it('bun < 1.1.0 or undefined → ok=false', async () => {
    {
      const { deps, bufs } = okDeps({ probeBun: vi.fn().mockReturnValueOnce('1.0.30') })
      const res = await runDoctorCommand(deps)
      expect(res.exitCode).toBe(1)
      expect(bufs.readOut()).toMatch(/✗ bun:.*requires bun >= 1\.1\.0/)
    }
    {
      const { deps, bufs } = okDeps({ probeBun: vi.fn().mockReturnValueOnce(undefined) })
      const res = await runDoctorCommand(deps)
      expect(res.exitCode).toBe(1)
      expect(bufs.readOut()).toMatch(/✗ bun:.*requires bun/)
    }
  })

  it('hid both channels available → ok=true, value=digitizer:true,indigo9:true', async () => {
    const { deps, bufs } = okDeps({ json: true })
    const res = await runDoctorCommand(deps)
    expect(res.exitCode).toBe(0)
    const parsed = JSON.parse(bufs.readOut().trim())
    expect(parsed.checks[4]).toMatchObject({
      name: 'hid',
      ok: true,
      value: { digitizer: true, indigo9: true },
    })
  })

  it('hid digitizer unavailable → ok=false, exitCode=1, line ✗ hid: digitizer ✗, indigo9 ✓', async () => {
    const { deps, bufs } = okDeps({
      probeHidChannels: vi
        .fn()
        .mockResolvedValue({ digitizer: false, indigo9: true }),
    })
    const res = await runDoctorCommand(deps)
    expect(res.exitCode).toBe(1)
    const lines = bufs.readOut().split('\n').filter(Boolean)
    expect(lines[4]).toMatch(/^✗ hid: digitizer ✗, indigo9 ✓$/)
  })

  it('hid both unavailable → ok=false, exitCode=1, line ✗ hid: digitizer ✗, indigo9 ✗', async () => {
    const { deps, bufs } = okDeps({
      probeHidChannels: vi
        .fn()
        .mockResolvedValue({ digitizer: false, indigo9: false }),
    })
    const res = await runDoctorCommand(deps)
    expect(res.exitCode).toBe(1)
    const lines = bufs.readOut().split('\n').filter(Boolean)
    expect(lines[4]).toMatch(/^✗ hid: digitizer ✗, indigo9 ✗$/)
  })

  it('hid probe rejects (binary not found) → ok=false, message contains probe error', async () => {
    const { deps, bufs } = okDeps({
      probeHidChannels: vi
        .fn()
        .mockRejectedValueOnce(
          new Error(
            'ENOENT: simx-host-hid not built (run: swift build --package-path swift-bridge)',
          ),
        ),
    })
    const res = await runDoctorCommand(deps)
    expect(res.exitCode).toBe(1)
    const out = bufs.readOut()
    expect(out).toMatch(/✗ hid:.*hid probe failed/)
    expect(out).toMatch(/simx-host-hid not built/)
  })

  // ---- v0.3 C2 axp check ----

  it('axp probe OK → check ok=true + 6 checks total + value has 4 top-level keys', async () => {
    const { deps, bufs } = okDeps({ json: true })
    const res = await runDoctorCommand(deps)
    expect(res.exitCode).toBe(0)
    const parsed = JSON.parse(bufs.readOut().trim())
    expect(parsed.checks).toHaveLength(6)
    const axp = parsed.checks[5]
    expect(axp.name).toBe('axp')
    expect(axp.ok).toBe(true)
    expect(Object.keys(axp.value).sort()).toEqual([
      'classes',
      'framework',
      'selectors',
      'sharedInstance',
    ])
  })

  it('axp probe framework not loaded → ok=false + JSON message contains "loaded:false"', async () => {
    const bufs = makeBufs()
    const { deps } = okDeps(
      {
        probeAxp: vi.fn().mockResolvedValue({
          framework: { path: AXP_FRAMEWORK_PATH, loaded: false },
          classes: { resolved: [], missing: [...AXP_CLASS_NAMES] },
          selectors: { resolved: [], missing: [...AXP_SELECTOR_NAMES] },
          sharedInstance: { available: false },
        }),
        json: true,
      },
      bufs,
    )
    const res = await runDoctorCommand(deps)
    expect(res.exitCode).toBe(1)
    const parsed = JSON.parse(bufs.readOut().trim())
    const axp = parsed.checks[5]
    expect(axp.name).toBe('axp')
    expect(axp.ok).toBe(false)
    expect(axp.message).toMatch(/loaded:false/)
    // Also verify the non-json human line on a separate run.
    const bufs2 = makeBufs()
    const { deps: deps2 } = okDeps(
      {
        probeAxp: vi.fn().mockResolvedValue({
          framework: { path: AXP_FRAMEWORK_PATH, loaded: false },
          classes: { resolved: [], missing: [...AXP_CLASS_NAMES] },
          selectors: { resolved: [], missing: [...AXP_SELECTOR_NAMES] },
          sharedInstance: { available: false },
        }),
      },
      bufs2,
    )
    await runDoctorCommand(deps2)
    expect(bufs2.readOut()).toMatch(/^✗ axp: framework ✗/m)
  })

  it('axp probe 1 class missing → ok=false + missing class appears in human line', async () => {
    const { deps, bufs } = okDeps({
      probeAxp: vi.fn().mockResolvedValue({
        framework: { path: AXP_FRAMEWORK_PATH, loaded: true },
        classes: {
          resolved: AXP_CLASS_NAMES.slice(1).map((s) => s),
          missing: ['AXPTranslator'],
        },
        selectors: { resolved: [...AXP_SELECTOR_NAMES], missing: [] },
        sharedInstance: { available: true },
      }),
    })
    const res = await runDoctorCommand(deps)
    expect(res.exitCode).toBe(1)
    const lines = bufs.readOut().split('\n').filter(Boolean)
    expect(lines[5]).toMatch(/^✗ axp:.*classes 4\/5/)
  })

  it('axp probe rejected → ok=false + exitCode=1 + other 5 checks still run', async () => {
    const { deps, bufs } = okDeps({
      probeAxp: vi.fn().mockRejectedValueOnce(
        new Error('ENOENT: simx-host-hid axp-probe binary not built'),
      ),
    })
    const res = await runDoctorCommand(deps)
    expect(res.exitCode).toBe(1)
    const lines = bufs.readOut().split('\n').filter(Boolean)
    expect(lines).toHaveLength(6)
    expect(lines[0]).toMatch(/^✓ xcode:/)
    expect(lines[1]).toMatch(/^✓ runtimes:/)
    expect(lines[2]).toMatch(/^✓ claude:/)
    expect(lines[3]).toMatch(/^✓ bun:/)
    expect(lines[4]).toMatch(/^✓ hid:/)
    expect(lines[5]).toMatch(/^✗ axp:.*axp probe failed.*simx-host-hid axp-probe/)
  })

  it('axp probe missing 1 selector → ok=false + selectors line shows 2/3', async () => {
    const { deps, bufs } = okDeps({
      probeAxp: vi.fn().mockResolvedValue({
        framework: { path: AXP_FRAMEWORK_PATH, loaded: true },
        classes: { resolved: [...AXP_CLASS_NAMES], missing: [] },
        selectors: {
          resolved: AXP_SELECTOR_NAMES.slice(0, 2).map((s) => s),
          missing: ['objectAtPoint:displayId:bridgeDelegateToken:'],
        },
        sharedInstance: { available: true },
      }),
    })
    const res = await runDoctorCommand(deps)
    expect(res.exitCode).toBe(1)
    const lines = bufs.readOut().split('\n').filter(Boolean)
    expect(lines[5]).toMatch(/^✗ axp:.*selectors 2\/3/)
  })

  it('axp probe text rendering shows ✓ icon when ok', async () => {
    const { deps, bufs } = okDeps()  // non-json
    const res = await runDoctorCommand(deps)
    expect(res.exitCode).toBe(0)
    const out = bufs.readOut()
    expect(out).toMatch(/✓ axp:/)
  })

  it('5 prior checks: xcode/bun/hid byte-identical to pre-C4; runtimes/claude deepened to C4 schema', async () => {
    // Regression gate: with all 6 probes happy, xcode/bun/hid must render
    // exactly as before C4; runtimes/claude now carry the C4 detail tail.
    const { deps, bufs } = okDeps()
    await runDoctorCommand(deps)
    const lines = bufs.readOut().split('\n').filter(Boolean)
    expect(lines[0]).toBe('✓ xcode: Xcode 26.4.1')
    expect(lines[1]).toBe('✓ runtimes: 3 (iOS 26.4, iOS 18.5, iOS 17.5)')
    expect(lines[2]).toBe('✓ claude: /usr/local/bin/claude (logged in)')
    expect(lines[3]).toBe('✓ bun: 1.3.13')
    expect(lines[4]).toBe('✓ hid: digitizer ✓, indigo9 ✓')
  })

  // ---- v0.5 C4 — runtimes list + claude logged-in deepening ----

  describe('C4: runtimes list + claude logged-in (v0.5 C4)', () => {
    it('runtimes JSON value includes count + typed items array', async () => {
      const { deps, bufs } = okDeps({ json: true })
      const res = await runDoctorCommand(deps)
      expect(res.exitCode).toBe(0)
      const parsed = JSON.parse(bufs.readOut().trim())
      const runtimes = parsed.checks[1]
      expect(runtimes.name).toBe('runtimes')
      expect(runtimes.ok).toBe(true)
      expect(runtimes.value.count).toBe(3)
      expect(runtimes.value.items).toHaveLength(3)
      for (const item of runtimes.value.items) {
        expect(typeof item.identifier).toBe('string')
        expect(typeof item.name).toBe('string')
        expect(typeof item.version).toBe('string')
        expect(typeof item.isAvailable).toBe('boolean')
      }
      expect(runtimes.value.items[0].name).toBe('iOS 26.4')
    })

    it('runtimes human renders count + all 3 names in parens', async () => {
      const { deps, bufs } = okDeps()
      await runDoctorCommand(deps)
      const lines = bufs.readOut().split('\n').filter(Boolean)
      expect(lines[1]).toBe('✓ runtimes: 3 (iOS 26.4, iOS 18.5, iOS 17.5)')
    })

    it('runtimes human truncates with ", ..." when > 5 items', async () => {
      const items = [
        { identifier: 'i26', name: 'iOS 26.4', version: '26.4', isAvailable: true },
        { identifier: 'i18', name: 'iOS 18.5', version: '18.5', isAvailable: true },
        { identifier: 'i17', name: 'iOS 17.5', version: '17.5', isAvailable: true },
        { identifier: 'i16', name: 'iOS 16.5', version: '16.5', isAvailable: true },
        { identifier: 'i15', name: 'iOS 15.5', version: '15.5', isAvailable: true },
        { identifier: 'i14', name: 'iOS 14.5', version: '14.5', isAvailable: true },
        { identifier: 'i13', name: 'iOS 13.5', version: '13.5', isAvailable: true },
      ]
      const { deps, bufs } = okDeps({
        probeRuntimes: vi.fn().mockResolvedValue({ count: items.length, items }),
      })
      await runDoctorCommand(deps)
      const lines = bufs.readOut().split('\n').filter(Boolean)
      expect(lines[1]).toBe(
        '✓ runtimes: 7 (iOS 26.4, iOS 18.5, iOS 17.5, iOS 16.5, iOS 15.5, ...)',
      )
    })

    it('claude loggedIn=true → ok + human "(logged in)"', async () => {
      const { deps, bufs } = okDeps({
        probeClaude: vi
          .fn()
          .mockResolvedValue({ path: '/usr/local/bin/claude', loggedIn: true }),
      })
      const res = await runDoctorCommand(deps)
      expect(res.exitCode).toBe(0)
      const lines = bufs.readOut().split('\n').filter(Boolean)
      expect(lines[2]).toBe('✓ claude: /usr/local/bin/claude (logged in)')
    })

    it('claude path found but loggedIn=false → ok=false + "ping failed" in JSON message', async () => {
      const bufs = makeBufs()
      const { deps } = okDeps(
        {
          probeClaude: vi.fn().mockResolvedValue({
            path: '/usr/local/bin/claude',
            loggedIn: false,
            failureDetail: 'command failed with exit 1',
          }),
          json: true,
        },
        bufs,
      )
      const res = await runDoctorCommand(deps)
      expect(res.exitCode).toBe(1)
      const parsed = JSON.parse(bufs.readOut().trim())
      const claude = parsed.checks[2]
      expect(claude.name).toBe('claude')
      expect(claude.ok).toBe(false)
      expect(claude.value).toEqual({
        path: '/usr/local/bin/claude',
        loggedIn: false,
        failureDetail: 'command failed with exit 1',
      })
      expect(claude.message).toMatch(/ping failed/)
      expect(claude.message).toMatch(/not logged in/)

      const bufs2 = makeBufs()
      const { deps: deps2 } = okDeps(
        {
          probeClaude: vi.fn().mockResolvedValue({
            path: '/usr/local/bin/claude',
            loggedIn: false,
            failureDetail: 'command failed with exit 1',
          }),
        },
        bufs2,
      )
      await runDoctorCommand(deps2)
      const lines = bufs2.readOut().split('\n').filter(Boolean)
      expect(lines[2]).toMatch(/^✗ claude: .*\(login check failed\)/)
    })

    it('claude path=undefined → ok=false + message "not found in PATH"', async () => {
      const { deps, bufs } = okDeps({
        probeClaude: vi
          .fn()
          .mockResolvedValue({ path: undefined, loggedIn: false }),
      })
      const res = await runDoctorCommand(deps)
      expect(res.exitCode).toBe(1)
      const lines = bufs.readOut().split('\n').filter(Boolean)
      expect(lines[2]).toMatch(/^✗ claude:.*not found in PATH/)
    })

    it('JSON schema CI-parsable: every check has name + ok + (value xor message)', async () => {
      const { deps, bufs } = okDeps({ json: true })
      await runDoctorCommand(deps)
      const parsed = JSON.parse(bufs.readOut().trim())
      expect(parsed.exitCode).toBe(0)
      expect(parsed.checks).toHaveLength(6)
      for (const c of parsed.checks) {
        expect(typeof c.name).toBe('string')
        expect(typeof c.ok).toBe('boolean')
        expect(c.value !== undefined || c.message !== undefined).toBe(true)
      }
    })

    it('check order locked: [xcode, runtimes, claude, bun, hid, axp]', async () => {
      const { deps, bufs } = okDeps({ json: true })
      await runDoctorCommand(deps)
      const parsed = JSON.parse(bufs.readOut().trim())
      expect(parsed.checks.map((c: { name: string }) => c.name)).toEqual([
        'xcode',
        'runtimes',
        'claude',
        'bun',
        'hid',
        'axp',
      ])
    })
  })

  describe('C4: compatibility field (v0.7 C4)', () => {
    it('every check has compatibility field with status enum + optional required/actual', async () => {
      const { deps, bufs } = okDeps({ json: true })
      const res = await runDoctorCommand(deps)
      expect(res.exitCode).toBe(0)
      const parsed = JSON.parse(bufs.readOut().trim())
      expect(parsed.checks).toHaveLength(6)
      const validStatus = ['supported', 'unsupported', 'partial', 'unknown']
      for (const c of parsed.checks) {
        expect(c.compatibility).toBeDefined()
        expect(typeof c.compatibility.status).toBe('string')
        expect(validStatus).toContain(c.compatibility.status)
      }
      const byName = Object.fromEntries(
        parsed.checks.map(
          (c: { name: string; compatibility: { required?: string } }) => [c.name, c.compatibility],
        ),
      )
      expect(byName.xcode.required).toMatch(/Xcode/)
      expect(byName.bun.required).toBe('>=1.1.0')
      expect(byName.claude.required).toBe('logged-in')
      expect(byName.hid.required).toMatch(/digitizer/)
    })

    it('compatibility status derived correctly for partial / unsupported scenarios', async () => {
      {
        const { deps, bufs } = okDeps({
          probeBun: vi.fn().mockReturnValueOnce('1.0.30'),
          json: true,
        })
        await runDoctorCommand(deps)
        const parsed = JSON.parse(bufs.readOut().trim())
        expect(parsed.checks[3].compatibility.status).toBe('partial')
        expect(parsed.checks[3].compatibility.actual).toBe('1.0.30')
      }
      {
        const { deps, bufs } = okDeps({
          probeRuntimes: vi.fn().mockResolvedValueOnce({ count: 0, items: [] }),
          json: true,
        })
        await runDoctorCommand(deps)
        const parsed = JSON.parse(bufs.readOut().trim())
        expect(parsed.checks[1].compatibility.status).toBe('unsupported')
      }
      {
        const { deps, bufs } = okDeps({
          probeClaude: vi
            .fn()
            .mockResolvedValueOnce({ path: undefined, loggedIn: false }),
          json: true,
        })
        await runDoctorCommand(deps)
        const parsed = JSON.parse(bufs.readOut().trim())
        expect(parsed.checks[2].compatibility.status).toBe('unsupported')
      }
      {
        const { deps, bufs } = okDeps({
          probeXcode: vi.fn().mockRejectedValueOnce(new Error('xcodebuild not found')),
          json: true,
        })
        await runDoctorCommand(deps)
        const parsed = JSON.parse(bufs.readOut().trim())
        expect(parsed.checks[0].compatibility.status).toBe('unknown')
      }
    })

    it('doctorReportSchema.parse round-trips a full --json output', async () => {
      const { doctorReportSchema } = await import('../commands/doctor-schemas.js')
      const { deps, bufs } = okDeps({ json: true })
      await runDoctorCommand(deps)
      const parsed = JSON.parse(bufs.readOut().trim())
      const result = doctorReportSchema.safeParse(parsed)
      expect(result.success).toBe(true)
      if (result.success) {
        expect(result.data.exitCode).toBe(0)
        expect(result.data.checks).toHaveLength(6)
      }
    })
  })
})
