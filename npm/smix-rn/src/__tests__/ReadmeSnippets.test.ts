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

describe('the README\'s "not wired up yet" warning is true', () => {
  it('names methods that really do throw SmixNotImplementedError', async () => {
    // The warning is the first thing an npm visitor reads. If a method
    // it names quietly started working (or was renamed), the page would
    // be lying in the direction that wastes the most of a reader's
    // time — so the claim is checked, not trusted.
    const named = [...README.matchAll(/`(Smix|App)\.(\w+)`/g)].map(
      (m) => [m[1], m[2]] as const,
    )
    expect(named.length).toBeGreaterThanOrEqual(3)

    const runtime = new HttpSimRuntime('http://127.0.0.1:1')
    const app = new App('com.example.app', runtime.resolver)
    for (const [holder, method] of named) {
      if (holder !== 'App') continue
      const fn = (app as unknown as Record<string, unknown>)[method as string]
      expect(typeof fn, `App.${method} is named by the README`).toBe('function')
      await expect(
        (fn as (...a: unknown[]) => Promise<unknown>).call(
          app,
          Selector.id('x'),
          'text',
        ),
        `App.${method} must throw the error the README promises`,
      ).rejects.toBeInstanceOf(SmixNotImplementedError)
    }
  })
})
