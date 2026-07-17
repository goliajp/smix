// The wire form of `smix-selector`'s untagged `Pattern` enum. Untagged
// means the shape alone says which case it is, so these forms are exact:
//
// Wire JSON forms (untagged):
//   Literal: bare string                   e.g. "hello"
//   Regex:   {"regex":"...","flags":"i"}    (flags default "i")

export type Pattern =
  | { kind: 'literal'; value: string }
  | { kind: 'regex'; regex: string; flags: string }

export function literal(value: string): Pattern {
  return { kind: 'literal', value }
}

export function regex(pat: string, flags = 'i'): Pattern {
  return { kind: 'regex', regex: pat, flags }
}

/**
 * Encode a Pattern to its Rust-compatible untagged JSON form
 * (bare string for literal / `{regex, flags}` object for regex).
 * Returns a JSON-encodable value (not a JSON string).
 */
export function patternToJson(p: Pattern): unknown {
  switch (p.kind) {
    case 'literal':
      return p.value
    case 'regex':
      return { regex: p.regex, flags: p.flags }
  }
}

/**
 * Decode a JSON value into a Pattern. Throws on unrecognized shape.
 */
export function patternFromJson(value: unknown): Pattern {
  if (typeof value === 'string') {
    return { kind: 'literal', value }
  }
  if (typeof value === 'object' && value !== null && 'regex' in value) {
    const obj = value as Record<string, unknown>
    const rx = obj.regex
    if (typeof rx !== 'string') {
      throw new Error(`Pattern.regex must be string, got: ${typeof rx}`)
    }
    const flags = typeof obj.flags === 'string' ? obj.flags : 'i'
    return { kind: 'regex', regex: rx, flags }
  }
  throw new Error(`Pattern must be string or {regex, flags?} object, got: ${JSON.stringify(value)}`)
}
