// The package README's code, type-checked and run.
//
// Nothing compiled these: the repo README's Rust snippet is now pinned
// by a compiled copy, but this package's four blocks — the ones an npm
// visitor reads first — had no such tie. The blocks below are the
// README's verbatim; the string test proves the README still says what
// this file compiles, and `tsc` proves this file matches the source.

import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import {
  App,
  ExpectationFailure,
  HttpSimRuntime,
  MockNodeDriver,
  MockSelectorResolver,
  Selector,
  Session,
  SmixNotImplementedError,
  literal,
  regex,
} from '../index'

const README = readFileSync(join(__dirname, '..', '..', 'README.md'), 'utf8')

/** The README's "Selector" block, verbatim. */
function selectorBlock(): void {
Selector.id('btn-login')
Selector.text(literal('Sign In'))
Selector.text(regex('^Sub', 'i'))
Selector.label('Settings')
Selector.role('button', literal('Submit'))
Selector.localizedText({ en: 'Submit', ja: '送信' })

// Fluent modifier chaining (returns a new Selector)
Selector.id('btn').below(Selector.text(literal('Address'))).nth(0)
Selector.role('button').near(Selector.text(literal('Confirm')))
}

/** The README's "Session lifecycle" block, verbatim. */
async function sessionBlock(): Promise<void> {
const runtime = new HttpSimRuntime('http://127.0.0.1:22087')
const session = await Session.open(runtime, 'com.example.app')
await session.relaunchApp()
await session.close()
}

describe('the README compiles', () => {
  it('has the selector block this file type-checks', () => {
    // Referencing them keeps the compiler honest about the block being
    // real code rather than dead text; neither needs a runner to exist.
    expect(typeof selectorBlock).toBe('function')
    expect(typeof sessionBlock).toBe('function')
  })

  it('every non-comment line of the README code appears in this file', () => {
    const thisFile = readFileSync(__filename, 'utf8')
    const blocks = [...README.matchAll(/```typescript\n([\s\S]*?)```/g)].map(
      (m) => m[1] ?? '',
    )
    expect(blocks.length).toBeGreaterThanOrEqual(3)
    const missing: string[] = []
    for (const block of blocks) {
      for (const line of block.split('\n')) {
        const t = line.trim()
        // Skip blank lines, imports (rewritten to a relative path
        // here), and the ExpectationFailure block's illustrative body.
        if (!t || t.startsWith('import') || t.startsWith('//')) continue
        if (t.startsWith('e.') || t === 'try {' || t === '} catch (e) {') continue
        if (t.startsWith('if (e instanceof') || t === '}') continue
        if (!thisFile.includes(t)) missing.push(t)
      }
    }
    expect(missing, 'README lines with no compiled counterpart').toEqual([])
  })
})

describe('the README\'s claims about this package', () => {
  it('exports ExpectationFailure with the documented shape', () => {
    const f = new ExpectationFailure({
      code: 'ELEMENT_NOT_FOUND',
      message: 'no such element',
    })
    // The README tells readers to reach for exactly these.
    expect(f.code).toBe('ELEMENT_NOT_FOUND')
    expect(Array.isArray(f.visibleElements)).toBe(true)
    expect(Array.isArray(f.suggestions)).toBe(true)
    expect(typeof f.toJson()).toBe('string')
    expect(f.toJson()).not.toContain('\n')
  })
})

describe('the README\'s throw-warning is true', () => {
  // The warning is the first thing an npm visitor reads. It must not lie in
  // either direction: a method it says still throws must throw, and a method
  // it shows driving must no longer throw SmixNotImplementedError.
  const makeApp = () => {
    const driver = new MockNodeDriver()
    return new App('com.example.app', driver, driver.session, new MockSelectorResolver().resolve)
  }

  it('names exactly the three surfaces that still throw, and they do', async () => {
    const gaps = new Set(
      [...README.matchAll(/`App\.(screenshot|openUrl|launchFresh)`/g)].map((m) => m[1]),
    )
    expect(gaps, 'README must name screenshot, openUrl, launchFresh as still-throwing').toEqual(
      new Set(['screenshot', 'openUrl', 'launchFresh']),
    )
    const app = makeApp()
    const calls: Array<() => Promise<unknown>> = [
      () => app.screenshot(),
      () => app.openUrl('https://x'),
      () => app.launchFresh(),
    ]
    for (const call of calls) {
      await expect(call()).rejects.toBeInstanceOf(SmixNotImplementedError)
    }
  })

  it('the driving methods the README shows working no longer throw napi', async () => {
    const app = makeApp()
    // Empty resolver -> tap fails ELEMENT_NOT_FOUND, a real failure, NOT
    // SmixNotImplementedError; snapshotTree drives and resolves.
    await expect(app.tap(Selector.id('x'))).rejects.not.toBeInstanceOf(SmixNotImplementedError)
    await expect(app.fill(Selector.id('x'), 't')).rejects.not.toBeInstanceOf(
      SmixNotImplementedError,
    )
    await expect(app.snapshotTree()).resolves.toBeDefined()
  })
})
