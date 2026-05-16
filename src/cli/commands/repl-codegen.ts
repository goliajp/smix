import { spawn } from 'node:child_process'
import type { ParsedCommand } from './repl-parser.js'
import type { Selector } from '../../core/index.js'

export type HistoryEntryLike = {
  seq: number
  raw: string
  parsed: ParsedCommand
}

export type TscValidation = { ok: true } | { ok: false; output: string }

const NAME_RE = /^[a-z0-9][a-z0-9-]{0,63}$/

export function isValidSaveName(name: string): boolean {
  return NAME_RE.test(name)
}

export function generateTestTs(
  entries: readonly HistoryEntryLike[],
  name: string,
): string {
  const lines: string[] = [
    "import { test } from '../src/sdk/index.js'",
    '',
    `test('${escapeSingleQuote(name)}', async ({ app }) => {`,
  ]
  for (const e of entries) {
    const stmt = renderAction(e.parsed)
    if (stmt !== null) lines.push(`  ${stmt}`)
  }
  lines.push('})')
  lines.push('')
  return lines.join('\n')
}

function renderAction(p: ParsedCommand): string | null {
  if (p.kind !== 'verb') return null
  switch (p.verb) {
    case 'launch':
      return `await app.launch('${escapeSingleQuote(p.arg)}')`
    case 'terminate':
      return `await app.terminate('${escapeSingleQuote(p.arg)}')`
    case 'tap':
      return `await app.tap(${renderSelector(p.selector)})`
    case 'screenshot':
      return 'await app.screenshot()'
    case 'describe':
      return null
  }
}

function renderSelector(s: Selector): string {
  const parts: string[] = []
  if ('text' in s && s.text !== undefined) {
    parts.push(`text: '${escapeSingleQuote(String(s.text))}'`)
  }
  if ('id' in s && s.id !== undefined) {
    parts.push(`id: '${escapeSingleQuote(s.id)}'`)
  }
  if ('label' in s && s.label !== undefined) {
    parts.push(`label: '${escapeSingleQuote(s.label)}'`)
  }
  if ('role' in s && s.role !== undefined) {
    parts.push(`role: '${escapeSingleQuote(s.role)}'`)
  }
  if ('name' in s && s.name !== undefined) {
    parts.push(`name: '${escapeSingleQuote(String(s.name))}'`)
  }
  return `{ ${parts.join(', ')} }`
}

function escapeSingleQuote(v: string): string {
  return v.replace(/\\/g, '\\\\').replace(/'/g, "\\'")
}

export async function validateGeneratedTs(
  absPath: string,
): Promise<TscValidation> {
  // absPath is currently observational — we run tsc over the whole
  // tsconfig.examples.json project so module resolution and strict
  // options match what user invocations of `bun x tsc` would see.
  void absPath
  return new Promise((resolve) => {
    const proc = spawn(
      'bun',
      ['x', 'tsc', '-p', 'tsconfig.examples.json', '--noEmit'],
      {
        cwd: process.cwd(),
        stdio: ['ignore', 'pipe', 'pipe'],
      },
    )
    let stdout = ''
    let stderr = ''
    proc.stdout.on('data', (b: Buffer) => {
      stdout += b.toString()
    })
    proc.stderr.on('data', (b: Buffer) => {
      stderr += b.toString()
    })
    proc.on('close', () => {
      const combined = stdout + stderr
      if (/error TS\d+/.test(combined)) {
        resolve({ ok: false, output: combined })
      } else {
        resolve({ ok: true })
      }
    })
    proc.on('error', (e) => {
      resolve({ ok: false, output: `tsc spawn failed: ${e.message}` })
    })
  })
}
