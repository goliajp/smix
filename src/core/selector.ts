import type { Role } from './role.js'

/**
 * Selector schema shared by SDK, CLI, MCP, and the driver wire format.
 *
 * Design rules:
 *  - Exactly one base form (text / id / label / role+name)
 *  - Optional modifiers stack on any base
 *  - No xpath, no raw coordinates — these surfaces are off-limits
 *    because AI authoring quality on them is poor
 */
export type Selector =
  | (BaseText & Modifiers)
  | (BaseId & Modifiers)
  | (BaseLabel & Modifiers)
  | (BaseRole & Modifiers)

export type BaseText = { text: string | RegExp }
export type BaseId = { id: string }
export type BaseLabel = { label: string }
export type BaseRole = { role: Role; name?: string | RegExp }

export type Modifiers = {
  near?: Selector
  below?: Selector
  above?: Selector
  leftOf?: Selector
  rightOf?: Selector
  inside?: Selector
  nth?: number
  first?: boolean
  last?: boolean
}

export function isTextSelector(s: Selector): s is BaseText & Modifiers {
  return 'text' in s && s.text !== undefined
}

export function isIdSelector(s: Selector): s is BaseId & Modifiers {
  return 'id' in s && typeof s.id === 'string'
}

export function isLabelSelector(s: Selector): s is BaseLabel & Modifiers {
  return 'label' in s && typeof s.label === 'string'
}

export function isRoleSelector(s: Selector): s is BaseRole & Modifiers {
  return 'role' in s && typeof s.role === 'string'
}

/**
 * Human / AI readable rendering of a selector for error prompts and logs.
 * Stable enough that AI can pattern-match against past failures.
 */
export function describeSelector(s: Selector): string {
  const parts: string[] = []

  if (isTextSelector(s)) {
    parts.push(`text=${formatPattern(s.text)}`)
  } else if (isIdSelector(s)) {
    parts.push(`id="${s.id}"`)
  } else if (isLabelSelector(s)) {
    parts.push(`label="${s.label}"`)
  } else if (isRoleSelector(s)) {
    if (s.name !== undefined) {
      parts.push(`role=${s.role}, name=${formatPattern(s.name)}`)
    } else {
      parts.push(`role=${s.role}`)
    }
  }

  if (s.near) parts.push(`near=(${describeSelector(s.near)})`)
  if (s.below) parts.push(`below=(${describeSelector(s.below)})`)
  if (s.above) parts.push(`above=(${describeSelector(s.above)})`)
  if (s.leftOf) parts.push(`leftOf=(${describeSelector(s.leftOf)})`)
  if (s.rightOf) parts.push(`rightOf=(${describeSelector(s.rightOf)})`)
  if (s.inside) parts.push(`inside=(${describeSelector(s.inside)})`)
  if (s.nth !== undefined) parts.push(`nth=${s.nth}`)
  if (s.first) parts.push('first')
  if (s.last) parts.push('last')

  return `{ ${parts.join(', ')} }`
}

function formatPattern(p: string | RegExp): string {
  return p instanceof RegExp ? p.toString() : JSON.stringify(p)
}
