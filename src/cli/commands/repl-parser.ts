import type { Selector, Role } from '../../core/index.js'

export type ParsedCommand =
  | { kind: 'noop' }
  | { kind: 'meta'; verb: '.exit' | '.help' | '.history' | '.undo' | '.redo' }
  | { kind: 'meta'; verb: '.save'; name: string }
  | { kind: 'verb'; verb: 'launch'; arg: string }
  | { kind: 'verb'; verb: 'terminate'; arg: string }
  | { kind: 'verb'; verb: 'screenshot' }
  | { kind: 'verb'; verb: 'describe' }
  | { kind: 'verb'; verb: 'tap'; selector: Selector }
  | { kind: 'error'; message: string }

const KNOWN_META: readonly string[] = ['.exit', '.help', '.history', '.undo', '.redo']

const MOD_KEYS: readonly string[] = [
  'near',
  'below',
  'above',
  'leftOf',
  'rightOf',
  'inside',
  'nth',
  'first',
  'last',
]

export function parseReplLine(raw: string): ParsedCommand {
  const line = raw.trim()
  if (line.length === 0) return { kind: 'noop' }

  if (line.startsWith('.')) {
    if (line === '.exit') return { kind: 'meta', verb: '.exit' }
    if (line === '.help') return { kind: 'meta', verb: '.help' }
    if (line === '.history') return { kind: 'meta', verb: '.history' }
    if (line === '.undo') return { kind: 'meta', verb: '.undo' }
    if (line === '.redo') return { kind: 'meta', verb: '.redo' }

    const metaTok = line.split(/\s+/)[0] ?? line
    if (metaTok === '.save') {
      const m = /^\.save\s+as\s+(\S+)\s*$/.exec(line)
      if (m && m[1] !== undefined) {
        return { kind: 'meta', verb: '.save', name: m[1] }
      }
      if (/^\.save\s+as\s*$/.test(line)) {
        return {
          kind: 'error',
          message: ".save requires a name after 'as' (e.g. .save as my-flow)",
        }
      }
      if (/^\.save\s+as\s+\S+\s+\S/.test(line)) {
        return {
          kind: 'error',
          message: '.save expects exactly one name argument',
        }
      }
      return {
        kind: 'error',
        message: ".save requires 'as <name>' (e.g. .save as my-flow)",
      }
    }
    if (KNOWN_META.includes(metaTok)) {
      return {
        kind: 'error',
        message: `${metaTok} takes no arguments`,
      }
    }
    return {
      kind: 'error',
      message: `unknown meta '${metaTok}'; type .help for list`,
    }
  }

  const m = /^(\S+)(?:\s+(.*))?$/.exec(line)
  if (!m) return { kind: 'error', message: `parse failure for '${line}'` }
  const verb = m[1] ?? ''
  const rest = (m[2] ?? '').trim()

  switch (verb) {
    case 'launch':
      if (rest.length === 0) {
        return { kind: 'error', message: 'launch requires a bundleId' }
      }
      return { kind: 'verb', verb: 'launch', arg: rest }
    case 'terminate':
      if (rest.length === 0) {
        return { kind: 'error', message: 'terminate requires a bundleId' }
      }
      return { kind: 'verb', verb: 'terminate', arg: rest }
    case 'screenshot':
      if (rest.length > 0) {
        return { kind: 'error', message: 'screenshot takes no arguments' }
      }
      return { kind: 'verb', verb: 'screenshot' }
    case 'describe':
      if (rest.length > 0) {
        return { kind: 'error', message: 'describe takes no arguments' }
      }
      return { kind: 'verb', verb: 'describe' }
    case 'tap':
      if (rest.length === 0) {
        return {
          kind: 'error',
          message: 'tap requires a selector (e.g. text=General)',
        }
      }
      return parseTapSelector(rest)
    default:
      return {
        kind: 'error',
        message: `unknown verb '${verb}'; type .help for list`,
      }
  }
}

function parseTapSelector(s: string): ParsedCommand {
  if (s.includes(',')) {
    const parts = s.split(',')
    const head = parts[0] ?? ''
    const tail = parts.slice(1).join(',')
    if (head.startsWith('role=')) {
      const roleVal = head.slice(5)
      if (roleVal.length === 0) {
        return { kind: 'error', message: 'bad selector: role= must have value' }
      }
      const nameMatch = /^name=(.*)$/.exec(tail)
      if (!nameMatch) {
        const modKey = (tail.split('=')[0] ?? '').trim()
        return {
          kind: 'error',
          message: `modifier not supported in C1: '${modKey}' (推 v0.5 C5)`,
        }
      }
      const nameVal = nameMatch[1] ?? ''
      if (nameVal.length === 0) {
        return { kind: 'error', message: 'bad selector: name= must have value' }
      }
      return {
        kind: 'verb',
        verb: 'tap',
        selector: { role: roleVal as Role, name: nameVal },
      }
    }
    const modKey = (tail.split('=')[0] ?? '').trim()
    return {
      kind: 'error',
      message: `modifier not supported in C1: '${modKey}' (推 v0.5 C5)`,
    }
  }

  const eq = s.indexOf('=')
  if (eq <= 0) {
    return {
      kind: 'error',
      message: `bad selector '${s}'; expected text=|id=|label=|role=`,
    }
  }
  const key = s.slice(0, eq)
  const val = s.slice(eq + 1)
  if (val.length === 0) {
    return { kind: 'error', message: `bad selector: ${key}= must have value` }
  }
  if (key === 'text') {
    return { kind: 'verb', verb: 'tap', selector: { text: val } }
  }
  if (key === 'id') {
    return { kind: 'verb', verb: 'tap', selector: { id: val } }
  }
  if (key === 'label') {
    return { kind: 'verb', verb: 'tap', selector: { label: val } }
  }
  if (key === 'role') {
    return { kind: 'verb', verb: 'tap', selector: { role: val as Role } }
  }
  if (MOD_KEYS.includes(key)) {
    return {
      kind: 'error',
      message: `modifier not supported in C1: '${key}' (推 v0.5 C5)`,
    }
  }
  return {
    kind: 'error',
    message: `bad selector '${s}'; expected text=|id=|label=|role=`,
  }
}
