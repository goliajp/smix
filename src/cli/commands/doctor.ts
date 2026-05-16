import type { WritableLike, CommandResult } from './list.js'
import type {
  Compatibility,
  RuntimesValue,
  ClaudeValue,
  HidChannelAvailability,
  AxpCapability,
  DoctorCheckName,
  DoctorCheck,
  DoctorReport,
} from './doctor-schemas.js'

export type {
  Compatibility,
  RuntimesValue,
  ClaudeValue,
  HidChannelAvailability,
  AxpCapability,
  DoctorCheckName,
  DoctorCheck,
  DoctorReport,
}

export type ProbeXcode = () => Promise<string>
export type ProbeRuntimes = () => Promise<RuntimesValue>
export type ProbeClaude = () => Promise<ClaudeValue>
export type ProbeBun = () => string | undefined
export type ProbeHidChannels = () => Promise<HidChannelAvailability>
export type ProbeAxp = () => Promise<AxpCapability>

export type RunDoctorDeps = {
  probeXcode: ProbeXcode
  probeRuntimes: ProbeRuntimes
  probeClaude: ProbeClaude
  probeBun: ProbeBun
  probeHidChannels: ProbeHidChannels
  probeAxp: ProbeAxp
  json: boolean
  out: WritableLike
  err: WritableLike
}

const MIN_BUN = '1.1.0'

function deriveCompatibility(
  name: DoctorCheckName,
  ok: boolean,
  value: DoctorCheck['value'],
  message: string | undefined,
  probeRejected: boolean,
): Compatibility {
  if (probeRejected) {
    return { status: 'unknown', ...(message !== undefined ? { message } : {}) }
  }
  switch (name) {
    case 'xcode': {
      const required = 'Xcode >=26.0'
      const v = typeof value === 'string' ? value : ''
      const m = /Xcode\s+([0-9]+(?:\.[0-9]+)*)/.exec(v)
      const actual = m?.[1] ?? v
      if (!ok || m === null) return { status: 'partial', required, actual }
      return gteSemver(m[1] ?? '0', '26.0')
        ? { status: 'supported', required, actual }
        : { status: 'partial', required, actual }
    }
    case 'runtimes': {
      const required = 'iOS 26.x present (17.5 / 18.4 nice-to-have)'
      if (typeof value === 'object' && value !== null && 'items' in value) {
        const rv = value as RuntimesValue
        if (rv.items.length === 0) {
          return { status: 'unsupported', required, actual: '0 available' }
        }
        const has26 = rv.items.some((r) => /iOS-26(?:-|$)/.test(r.identifier))
        const head = rv.items.slice(0, 5).map((r) => r.identifier).join(', ')
        const suffix = rv.items.length > 5 ? ', ...' : ''
        const actual = `${rv.items.length} available: ${head}${suffix}`
        return { status: has26 ? 'supported' : 'partial', required, actual }
      }
      return { status: 'unsupported', required, actual: '0 available' }
    }
    case 'claude': {
      const required = 'logged-in'
      if (typeof value === 'object' && value !== null && 'loggedIn' in value) {
        const cv = value as ClaudeValue
        if (cv.path === undefined) {
          return { status: 'unsupported', required, actual: 'missing' }
        }
        return cv.loggedIn
          ? { status: 'supported', required, actual: 'logged-in' }
          : { status: 'partial', required, actual: 'not-logged-in' }
      }
      return { status: 'unsupported', required, actual: 'missing' }
    }
    case 'bun': {
      const required = `>=${MIN_BUN}`
      const v = typeof value === 'string' ? value : undefined
      if (v === undefined) return { status: 'unsupported', required, actual: 'undefined' }
      return gteSemver(v, MIN_BUN)
        ? { status: 'supported', required, actual: v }
        : { status: 'partial', required, actual: v }
    }
    case 'hid': {
      const required = 'digitizer + indigo9'
      if (typeof value === 'object' && value !== null && 'digitizer' in value) {
        const hv = value as HidChannelAvailability
        const actual = `digitizer=${hv.digitizer}, indigo9=${hv.indigo9}`
        if (hv.digitizer && hv.indigo9) return { status: 'supported', required, actual }
        if (hv.digitizer || hv.indigo9) return { status: 'partial', required, actual }
        return { status: 'unsupported', required, actual }
      }
      return { status: 'unsupported', required, actual: 'no channels' }
    }
    case 'axp': {
      const required = 'framework + 5 classes + 3 selectors + +sharedInstance'
      if (typeof value === 'object' && value !== null && 'framework' in value) {
        const av = value as AxpCapability
        const cTot = av.classes.resolved.length + av.classes.missing.length
        const sTot = av.selectors.resolved.length + av.selectors.missing.length
        const actual =
          `framework=${av.framework.loaded}, classes=${av.classes.resolved.length}/${cTot}, ` +
          `selectors=${av.selectors.resolved.length}/${sTot}, ` +
          `sharedInstance=${av.sharedInstance.available}`
        const allGood =
          av.framework.loaded &&
          av.classes.missing.length === 0 &&
          av.selectors.missing.length === 0 &&
          av.sharedInstance.available
        if (allGood) return { status: 'supported', required, actual }
        if (av.framework.loaded) return { status: 'partial', required, actual }
        return { status: 'unsupported', required, actual }
      }
      return { status: 'unsupported', required, actual: 'no axp data' }
    }
  }
}

export async function runDoctorCommand(deps: RunDoctorDeps): Promise<CommandResult> {
  const checks: DoctorCheck[] = [
    await checkXcode(deps.probeXcode),
    await checkRuntimes(deps.probeRuntimes),
    await checkClaude(deps.probeClaude),
    checkBun(deps.probeBun),
    await checkHid(deps.probeHidChannels),
    await checkAxp(deps.probeAxp),
  ]
  const exitCode: 0 | 1 = checks.every((c) => c.ok) ? 0 : 1

  if (deps.json) {
    deps.out.write(`${JSON.stringify({ exitCode, checks })}\n`)
  } else {
    for (const c of checks) {
      const icon = c.ok ? '✓' : '✗'
      const tail = renderTail(c)
      deps.out.write(`${icon} ${c.name}: ${tail}\n`)
    }
  }
  return { exitCode }
}

function renderTail(c: DoctorCheck): string {
  if (c.name === 'hid') {
    if (typeof c.value === 'object' && c.value !== null && 'digitizer' in c.value) {
      return hidHumanLine(c.value as HidChannelAvailability)
    }
    return c.message ?? 'failed'
  }
  if (c.name === 'axp') {
    if (typeof c.value === 'object' && c.value !== null && 'framework' in c.value) {
      return axpHumanLine(c.value as AxpCapability)
    }
    return c.message ?? 'failed'
  }
  if (c.name === 'runtimes') {
    if (typeof c.value === 'object' && c.value !== null && 'items' in c.value) {
      const rv = c.value as RuntimesValue
      return c.ok ? runtimesHumanLine(rv) : (c.message ?? 'failed')
    }
    return c.message ?? 'failed'
  }
  if (c.name === 'claude') {
    if (typeof c.value === 'object' && c.value !== null && 'loggedIn' in c.value) {
      return claudeHumanLine(c.value as ClaudeValue)
    }
    return c.message ?? 'failed'
  }
  // Existing string-tail path (xcode / bun).
  const v = typeof c.value === 'string' ? c.value : undefined
  return c.ok ? (v ?? '') : (c.message ?? 'failed')
}

function runtimesHumanLine(v: RuntimesValue): string {
  const HEAD = 5
  if (v.items.length === 0) return `${v.count}`
  const names = v.items.slice(0, HEAD).map((r) => r.name)
  const suffix = v.items.length > HEAD ? ', ...' : ''
  return `${v.count} (${names.join(', ')}${suffix})`
}

function claudeHumanLine(v: ClaudeValue): string {
  if (v.path === undefined) return 'not found in PATH'
  return v.loggedIn ? `${v.path} (logged in)` : `${v.path} (login check failed)`
}

function hidHumanLine(v: HidChannelAvailability): string {
  const d = v.digitizer ? '✓' : '✗'
  const i = v.indigo9 ? '✓' : '✗'
  return `digitizer ${d}, indigo9 ${i}`
}

function axpHumanLine(v: AxpCapability): string {
  const fw = v.framework.loaded ? '✓' : '✗'
  const totalClasses = v.classes.resolved.length + v.classes.missing.length
  const totalSelectors = v.selectors.resolved.length + v.selectors.missing.length
  const si = v.sharedInstance.available ? '✓' : '✗'
  return (
    `framework ${fw}, classes ${v.classes.resolved.length}/${totalClasses}, ` +
    `selectors ${v.selectors.resolved.length}/${totalSelectors}, ` +
    `+sharedInstance ${si}`
  )
}

async function checkXcode(probe: ProbeXcode): Promise<DoctorCheck> {
  try {
    const line = (await probe()).trim()
    if (/^Xcode \d+\./.test(line)) {
      return { name: 'xcode', ok: true, value: line, compatibility: deriveCompatibility('xcode', true, line, undefined, false) }
    }
    const message = `unexpected output: ${line}`
    return { name: 'xcode', ok: false, value: line, message, compatibility: deriveCompatibility('xcode', false, line, message, false) }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    const message = `xcodebuild failed: ${msg}`
    return { name: 'xcode', ok: false, message, compatibility: deriveCompatibility('xcode', false, undefined, message, true) }
  }
}

async function checkRuntimes(probe: ProbeRuntimes): Promise<DoctorCheck> {
  try {
    const v = await probe()
    if (v.items.length >= 1) {
      return { name: 'runtimes', ok: true, value: v, compatibility: deriveCompatibility('runtimes', true, v, undefined, false) }
    }
    const message = 'no iOS runtime installed'
    return { name: 'runtimes', ok: false, value: v, message, compatibility: deriveCompatibility('runtimes', false, v, message, false) }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    const message = `simctl list runtimes failed: ${msg}`
    return { name: 'runtimes', ok: false, message, compatibility: deriveCompatibility('runtimes', false, undefined, message, true) }
  }
}

async function checkClaude(probe: ProbeClaude): Promise<DoctorCheck> {
  try {
    const v = await probe()
    if (v.path === undefined) {
      const message = 'claude not found in PATH (install Claude Code)'
      return { name: 'claude', ok: false, message, compatibility: deriveCompatibility('claude', false, v, message, false) }
    }
    if (!v.loggedIn) {
      const tail = v.failureDetail !== undefined && v.failureDetail.length > 0
        ? `: ${v.failureDetail}`
        : ''
      const message = `found at ${v.path} but ping failed (not logged in?)${tail}`
      return { name: 'claude', ok: false, value: v, message, compatibility: deriveCompatibility('claude', false, v, message, false) }
    }
    return { name: 'claude', ok: true, value: v, compatibility: deriveCompatibility('claude', true, v, undefined, false) }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    const message = `claude probe failed: ${msg}`
    return { name: 'claude', ok: false, message, compatibility: deriveCompatibility('claude', false, undefined, message, true) }
  }
}

function checkBun(probe: ProbeBun): DoctorCheck {
  const v = probe()
  if (v === undefined) {
    const message = `not running under bun; requires bun >= ${MIN_BUN}`
    return { name: 'bun', ok: false, message, compatibility: deriveCompatibility('bun', false, undefined, message, false) }
  }
  if (gteSemver(v, MIN_BUN)) {
    return { name: 'bun', ok: true, value: v, compatibility: deriveCompatibility('bun', true, v, undefined, false) }
  }
  const message = `requires bun >= ${MIN_BUN}, got ${v}`
  return { name: 'bun', ok: false, value: v, message, compatibility: deriveCompatibility('bun', false, v, message, false) }
}

async function checkHid(probe: ProbeHidChannels): Promise<DoctorCheck> {
  try {
    const v = await probe()
    const ok = v.digitizer && v.indigo9
    if (ok) {
      return { name: 'hid', ok: true, value: v, compatibility: deriveCompatibility('hid', true, v, undefined, false) }
    }
    const message = `hid channels unavailable: ${hidHumanLine(v)}`
    return { name: 'hid', ok: false, value: v, message, compatibility: deriveCompatibility('hid', false, v, message, false) }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    const message = `hid probe failed: ${msg}`
    return { name: 'hid', ok: false, message, compatibility: deriveCompatibility('hid', false, undefined, message, true) }
  }
}

async function checkAxp(probe: ProbeAxp): Promise<DoctorCheck> {
  try {
    const v = await probe()
    const ok =
      v.framework.loaded &&
      v.classes.missing.length === 0 &&
      v.selectors.missing.length === 0 &&
      v.sharedInstance.available
    if (ok) {
      return { name: 'axp', ok: true, value: v, compatibility: deriveCompatibility('axp', true, v, undefined, false) }
    }
    const reasons: string[] = []
    if (!v.framework.loaded) {
      reasons.push(`framework loaded:false (path=${v.framework.path})`)
    }
    if (v.classes.missing.length > 0) {
      reasons.push(`classes missing: ${v.classes.missing.join(', ')}`)
    }
    if (v.selectors.missing.length > 0) {
      reasons.push(`selectors missing: ${v.selectors.missing.join(', ')}`)
    }
    if (!v.sharedInstance.available) {
      reasons.push('+sharedInstance unavailable')
    }
    const message = `axp probe failed: ${reasons.join('; ')}`
    return { name: 'axp', ok: false, value: v, message, compatibility: deriveCompatibility('axp', false, v, message, false) }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    const message = `axp probe failed: ${msg}`
    return { name: 'axp', ok: false, message, compatibility: deriveCompatibility('axp', false, undefined, message, true) }
  }
}

function gteSemver(a: string, b: string): boolean {
  const pa = a.split('.').map((x) => Number.parseInt(x, 10))
  const pb = b.split('.').map((x) => Number.parseInt(x, 10))
  for (let i = 0; i < 3; i += 1) {
    const ai = pa[i] ?? 0
    const bi = pb[i] ?? 0
    if (Number.isNaN(ai) || Number.isNaN(bi)) return false
    if (ai > bi) return true
    if (ai < bi) return false
  }
  return true
}
