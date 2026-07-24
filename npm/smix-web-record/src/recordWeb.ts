// The Playwright bridge: capture browser interactions passively and map them
// to IRAction. Injection is the web equivalent of the native AX streams —
// addInitScript installs a capture-phase click/input listener, exposeBinding
// forwards each event to Node, and mapDomEvents turns them into IRAction.
//
// Per §9#1 this is a driver-layer DOM bridge over headless chromium, not a
// physical device.

import { chromium } from 'playwright'
import type { Page } from 'playwright'
import { mapDomEvents, type CapturedDomEvent } from './mapDomEvents.js'

/** The capture script, stringified into the page. Self-contained: it may only
 *  reference the page globals and the exposed `__smixCapture` binding. */
function captureScript(): void {
  const before = new WeakMap<Element, string>()
  const attr = (el: Element | null, name: string) => el?.getAttribute?.(name) ?? undefined
  const send = (e: Record<string, unknown>) =>
    (window as unknown as { __smixCapture: (e: unknown) => void }).__smixCapture(e)

  document.addEventListener(
    'click',
    (ev) => {
      const el = ev.target as Element | null
      send({
        kind: 'click',
        testId: attr(el, 'data-testid'),
        role: attr(el, 'role'),
        text: (el?.textContent ?? '').trim() || undefined,
        timestampMs: Date.now(),
      })
    },
    true,
  )
  document.addEventListener(
    'input',
    (ev) => {
      const el = ev.target as (Element & { value?: string }) | null
      if (!el) return
      const value = el.value ?? ''
      send({
        kind: 'input',
        testId: attr(el, 'data-testid'),
        role: attr(el, 'role'),
        value,
        beforeValue: before.get(el) ?? '',
        timestampMs: Date.now(),
      })
      before.set(el, value)
    },
    true,
  )
}

/**
 * Record a web session: open `url`, run `drive(page)` (the interactions to
 * capture), and return the IRAction JSON the interactions map to.
 */
export async function recordWebSession(
  url: string,
  drive: (page: Page) => Promise<void>,
): Promise<string[]> {
  const events: CapturedDomEvent[] = []
  const browser = await chromium.launch()
  try {
    const page = await browser.newPage()
    await page.exposeBinding('__smixCapture', (_src, e: CapturedDomEvent) => {
      events.push(e)
    })
    await page.addInitScript(captureScript)
    await page.goto(url)
    await drive(page)
    // Let the last input/click events flush to Node.
    await page.waitForTimeout(100)
    return mapDomEvents(events).actions
  } finally {
    await browser.close()
  }
}
