// CapturedDomEvent -> IRAction, the web capture leg of the cross-platform
// recorder (v2.10-C3). The Playwright bridge injects a capture-phase listener
// (addInitScript) that forwards click/input events to Node via exposeBinding;
// this maps them to the same IRAction JSON the Android RecordMapper emits.
//
// Pure — no Playwright import — so it is browser-free and vitest-tested. The
// coalescing / fill-vs-clear / drop-and-count logic mirrors the Android leg.

/** A DOM interaction captured in-page and forwarded to Node. */
export interface CapturedDomEvent {
  kind: 'click' | 'input'
  /** `data-testid` — the stable, author-declared target id (main path). */
  testId?: string
  /** Computed ARIA role, when the element has one. */
  role?: string
  /** Visible text content (click targets). */
  text?: string
  /** Current value (input events). */
  value?: string
  /** Value before this input (input events), for fill-vs-clear. */
  beforeValue?: string
  timestampMs: number
}

export interface WebMapResult {
  actions: string[]
  unmapped: number
}

// ARIA role -> smix Role. smix's vocabulary is iOS-centric, so several web
// roles (textbox / combobox / listbox / option / spinbutton / searchbox) have
// no clean peer and are left unmapped rather than faked.
const ROLE_MAP: Readonly<Record<string, string>> = {
  button: 'button',
  link: 'link',
  checkbox: 'checkBox',
  radio: 'radio',
  switch: 'switch',
  tab: 'tab',
  menu: 'menu',
  menuitem: 'menuItem',
  dialog: 'dialog',
  alert: 'alert',
  slider: 'slider',
  img: 'image',
  image: 'image',
  table: 'table',
  heading: 'staticText',
}

/** Resolve a selector for an event, most-stable first; null if none maps. */
function selectorFor(e: CapturedDomEvent): Record<string, string> | null {
  if (e.testId) return { id: e.testId }
  if (e.role !== undefined && ROLE_MAP[e.role] !== undefined) {
    return { role: ROLE_MAP[e.role] as string }
  }
  if (e.text) return { text: e.text }
  return null
}

/** A stable key for coalescing consecutive inputs on the same target. */
function targetKey(sel: Record<string, string>): string {
  return JSON.stringify(sel)
}

export function mapDomEvents(events: readonly CapturedDomEvent[]): WebMapResult {
  const actions: string[] = []
  let unmapped = 0
  let i = 0
  while (i < events.length) {
    const e = events[i] as CapturedDomEvent
    const sel = selectorFor(e)
    if (sel === null) {
      unmapped++
      i++
      continue
    }
    if (e.kind === 'click') {
      actions.push(json('tap', sel, undefined, e.timestampMs))
      i++
    } else {
      // input: coalesce a run of same-target inputs to the last (final value).
      const key = targetKey(sel)
      let j = i
      while (j + 1 < events.length) {
        const n = events[j + 1] as CapturedDomEvent
        if (n.kind !== 'input') break
        const ns = selectorFor(n)
        if (ns === null || targetKey(ns) !== key) break
        j++
      }
      const last = events[j] as CapturedDomEvent
      const value = last.value ?? ''
      if (value === '' && last.beforeValue !== undefined && last.beforeValue !== '') {
        actions.push(json('clear', sel, undefined, last.timestampMs))
      } else {
        actions.push(json('fill', sel, value, last.timestampMs))
      }
      i = j + 1
    }
  }
  return { actions, unmapped }
}

function json(
  kind: string,
  selector: Record<string, string>,
  text: string | undefined,
  timestampMs: number,
): string {
  const obj: Record<string, unknown> = { kind, selector, timestampMs }
  if (text !== undefined) obj.text = text
  return JSON.stringify(obj)
}
