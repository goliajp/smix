import type { Selector } from './selector.js'
import { describeSelector } from './selector.js'
import type { ElementSummary } from './screen.js'
import type { FailureCode, SerializedFailure } from './schemas.js'

export type { FailureCode, SerializedFailure } from './schemas.js'

export type FailureInit = {
  code: FailureCode
  message: string
  selector?: Selector
  suggestions?: string[]
  visibleElements?: ElementSummary[]
  /** base64-encoded PNG, omitted from default toString to keep logs lean */
  screenshot?: string
  hint?: string
}

/**
 * Structured failure thrown by SDK matchers and driver calls.
 *
 * Critically: this is shaped to be fed back to an AI as a fix prompt,
 * not to be read by humans. toPrompt() emits a deterministic, AI-friendly
 * rendering with visible elements and suggestions inline.
 */
export class ExpectationFailure extends Error {
  readonly code: FailureCode
  readonly selector: Selector | undefined
  readonly suggestions: string[]
  readonly visibleElements: ElementSummary[]
  readonly screenshot: string | undefined
  readonly hint: string | undefined

  constructor(init: FailureInit) {
    super(init.message)
    this.name = 'ExpectationFailure'
    this.code = init.code
    this.selector = init.selector
    this.suggestions = init.suggestions ?? []
    this.visibleElements = init.visibleElements ?? []
    this.screenshot = init.screenshot
    this.hint = init.hint
  }

  /**
   * AI-facing rendering. Designed so the output can be pasted back as
   * a user message into a coding agent and the agent can act on it
   * without further context.
   */
  toPrompt(): string {
    const lines: string[] = []
    lines.push(`FAIL [${this.code}]: ${this.message}`)

    if (this.selector) {
      lines.push(`  selector: ${describeSelector(this.selector)}`)
    }

    if (this.suggestions.length > 0) {
      lines.push(`  suggestions:`)
      for (const s of this.suggestions) lines.push(`    - ${s}`)
    }

    if (this.visibleElements.length > 0) {
      lines.push(`  visible elements (top ${Math.min(10, this.visibleElements.length)}):`)
      for (const el of this.visibleElements.slice(0, 10)) {
        lines.push(`    - ${renderElement(el)}`)
      }
    }

    if (this.hint) {
      lines.push(`  hint: ${this.hint}`)
    }

    return lines.join('\n')
  }

  /**
   * Serializable form for MCP responses and trace files.
   * Excludes screenshot by default — pass includeScreenshot when needed.
   */
  toJSON(includeScreenshot = false): SerializedFailure {
    const out: SerializedFailure = {
      ok: false,
      code: this.code,
      message: this.message,
      suggestions: this.suggestions,
      visibleElements: this.visibleElements,
    }
    if (this.selector) out.selector = this.selector
    if (this.hint) out.hint = this.hint
    if (includeScreenshot && this.screenshot) out.screenshot = this.screenshot
    return out
  }

  override toString(): string {
    return this.toPrompt()
  }
}

function renderElement(el: ElementSummary): string {
  const bits: string[] = [el.role]
  if (el.name) bits.push(`name=${JSON.stringify(el.name)}`)
  if (el.id) bits.push(`id="${el.id}"`)
  if (el.text && el.text !== el.name) bits.push(`text=${JSON.stringify(el.text)}`)
  if (!el.enabled) bits.push('disabled')
  return bits.join(' ')
}
