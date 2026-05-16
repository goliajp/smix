import { createInterface } from 'node:readline'
import { promises as fs } from 'node:fs'
import * as path from 'node:path'
import type { Readable } from 'node:stream'
import type { App } from '../../sdk/index.js'
import { ExpectationFailure } from '../../core/index.js'
import type { ScreenDescription, ElementSummary } from '../../core/index.js'
import { TapNotFoundError, RunnerTransportError } from '../../sim/runner-client.js'
import type { Cell } from '../../sim/cell.js'
import { releaseCell } from '../../sim/cell.js'
import type { WritableLike, CommandResult } from './list.js'
import { parseReplLine, type ParsedCommand } from './repl-parser.js'
import {
  generateTestTs,
  isValidSaveName,
  validateGeneratedTs,
} from './repl-codegen.js'

export type RunReplDeps = {
  app: App
  cell: Cell
  input: Readable
  output: WritableLike
  err: WritableLike
}

type HistoryEntry = {
  seq: number
  raw: string
  parsed: ParsedCommand
}

type HistoryOps = {
  renderHistory: () => void
  doUndo: () => void
  doRedo: () => void
  doSave: (name: string) => Promise<void>
}

type DispatchOutcome =
  | { kind: 'noop' }
  | { kind: 'parser-error' }
  | { kind: 'dispatch-error' }
  | { kind: 'meta-only' }
  | { kind: 'action-success' }

const MAX_HISTORY = 1000

const HELP = [
  'simx repl commands:',
  '  launch <bundleId>          launch app by bundle id',
  '  terminate <bundleId>       terminate app',
  '  tap <selector>             tap element; selector = text=X | id=X | label=X | role=X[,name=Y]',
  '  screenshot                 capture screen PNG',
  '  describe                   print top-5 visible elements',
  '  .history                   show command history (undo + redo)',
  '  .undo                      pop last command from history (edits draft, no sim rollback)',
  '  .redo                      push last undone command back',
  '  .save as <name>            generate examples/<name>.test.ts from undo history + tsc check',
  '  .help                      this list',
  '  .exit                      quit (or ctrl-D)',
].join('\n')

export async function runReplCommand(deps: RunReplDeps): Promise<CommandResult> {
  const { app, cell, input, output, err } = deps
  output.write(`simx repl — Cell ${cell.id} @ ${cell.udid}\n`)
  output.write(`Driver: SimctlDriver (runner :${cell.runnerPort})\n`)
  output.write('Type .help for commands, .exit to quit.\n')

  // readline writes prompts/echoes to its output. We route those into our
  // WritableLike so tests can capture them; terminal:false avoids ANSI ops.
  const rlOutput: NodeJS.WritableStream = {
    write: (chunk: unknown): boolean => {
      output.write(typeof chunk === 'string' ? chunk : String(chunk))
      return true
    },
    end: () => {},
  } as unknown as NodeJS.WritableStream

  const rl = createInterface({
    input,
    output: rlOutput,
    prompt: '> ',
    terminal: false,
  })

  let exiting = false
  let rlClosed = false
  // Serialize line handling: readline may emit multiple 'line' events
  // before our async dispatch resolves, and we need each command (and its
  // auto-describe) to finish before the next one runs to keep behavior
  // deterministic for tests and for AI consumers reading the stream.
  let pending: Promise<void> = Promise.resolve()

  const undoStack: HistoryEntry[] = []
  const redoStack: HistoryEntry[] = []
  let nextSeq = 1

  function pushHistory(parsed: ParsedCommand, raw: string): void {
    undoStack.push({ seq: nextSeq++, raw, parsed })
    redoStack.length = 0
    if (undoStack.length > MAX_HISTORY) undoStack.shift()
  }

  function renderHistory(): void {
    const total = undoStack.length + redoStack.length
    if (total === 0) {
      output.write('no history yet\n')
      return
    }
    output.write(`history (${total} entries):\n`)
    const merged = [...undoStack, ...redoStack].sort((a, b) => a.seq - b.seq)
    for (const e of merged) {
      const isRedo = redoStack.includes(e)
      const prefix = isRedo ? '(redo) ' : ''
      output.write(`  ${e.seq}. ${prefix}${e.raw}\n`)
    }
  }

  function doUndo(): void {
    const last = undoStack.pop()
    if (last === undefined) {
      output.write('nothing to undo\n')
      return
    }
    redoStack.push(last)
    output.write(`← undo: ${last.raw}\n`)
  }

  function doRedo(): void {
    const last = redoStack.pop()
    if (last === undefined) {
      output.write('nothing to redo\n')
      return
    }
    undoStack.push(last)
    output.write(`→ redo: ${last.raw}\n`)
  }

  async function doSave(name: string): Promise<void> {
    if (!isValidSaveName(name)) {
      output.write(
        `error: bad name '${name}'; allowed: [a-z0-9][a-z0-9-]{0,63}\n`,
      )
      return
    }
    if (undoStack.length === 0) {
      output.write('nothing to save (history is empty)\n')
      return
    }
    const absPath = path.resolve(process.cwd(), 'examples', `${name}.test.ts`)
    let exists = false
    try {
      await fs.access(absPath)
      exists = true
    } catch {
      // ENOENT expected when file does not yet exist; proceed
    }
    if (exists) {
      output.write(
        `error: examples/${name}.test.ts already exists; pick another name\n`,
      )
      return
    }
    const content = generateTestTs(undoStack, name)
    try {
      await fs.writeFile(absPath, content, 'utf8')
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      output.write(`error: write failed: ${msg}\n`)
      return
    }
    const result = await validateGeneratedTs(absPath)
    if (result.ok) {
      output.write(`→ saved: examples/${name}.test.ts (tsc ok)\n`)
    } else {
      output.write(
        `warning: examples/${name}.test.ts saved but tsc reported errors:\n`,
      )
      const lines = result.output.split('\n').slice(0, 10)
      for (const l of lines) output.write(`  ${l}\n`)
      output.write('  (file kept on disk for inspection)\n')
    }
  }

  const historyOps: HistoryOps = { renderHistory, doUndo, doRedo, doSave }

  await new Promise<void>((resolve) => {
    rl.on('line', (raw) => {
      pending = pending.then(async () => {
        try {
          const trimmed = raw.trim()
          const parsed = parseReplLine(raw)
          let outcome: DispatchOutcome
          try {
            outcome = await dispatch(parsed, app, output, err, historyOps)
          } catch (e) {
            const msg = e instanceof Error ? e.message : String(e)
            err.write(`error: unexpected: ${msg}\n`)
            outcome = { kind: 'dispatch-error' }
          }
          if (outcome.kind === 'action-success') {
            pushHistory(parsed, trimmed)
          }
          if (parsed.kind === 'meta' && parsed.verb === '.exit') {
            exiting = true
            if (!rlClosed) {
              rlClosed = true
              rl.close()
            }
            return
          }
          if (!exiting && !rlClosed) {
            try {
              rl.prompt()
            } catch {
              // rl may have been closed by the input stream ending mid-dispatch
              rlClosed = true
            }
          }
        } catch (e) {
          // never let a per-line failure break the serialization chain;
          // subsequent queued lines must still get to run
          const msg = e instanceof Error ? e.message : String(e)
          err.write(`error: unexpected: ${msg}\n`)
        }
      })
    })
    rl.on('close', () => {
      rlClosed = true
      pending.finally(() => resolve())
    })
    rl.prompt()
  })

  try {
    await releaseCell(cell, { shutdown: false })
  } catch {
    // silent: release errors must not change exit code on graceful quit
  }
  return { exitCode: 0 }
}

async function dispatch(
  cmd: ParsedCommand,
  app: App,
  output: WritableLike,
  err: WritableLike,
  ops: HistoryOps,
): Promise<DispatchOutcome> {
  switch (cmd.kind) {
    case 'noop':
      return { kind: 'noop' }
    case 'error':
      err.write(`error: ${cmd.message}\n`)
      return { kind: 'parser-error' }
    case 'meta':
      switch (cmd.verb) {
        case '.help':
          output.write(HELP + '\n')
          return { kind: 'meta-only' }
        case '.history':
          ops.renderHistory()
          return { kind: 'meta-only' }
        case '.undo':
          ops.doUndo()
          return { kind: 'meta-only' }
        case '.redo':
          ops.doRedo()
          return { kind: 'meta-only' }
        case '.save':
          await ops.doSave(cmd.name)
          return { kind: 'meta-only' }
        case '.exit':
          return { kind: 'meta-only' }
      }
    // eslint-disable-next-line no-fallthrough
    case 'verb':
      return await runVerb(cmd, app, output, err)
  }
}

async function runVerb(
  cmd: Extract<ParsedCommand, { kind: 'verb' }>,
  app: App,
  output: WritableLike,
  err: WritableLike,
): Promise<DispatchOutcome> {
  try {
    switch (cmd.verb) {
      case 'launch':
        await app.launch(cmd.arg)
        break
      case 'terminate':
        await app.terminate(cmd.arg)
        break
      case 'tap':
        await app.tap(cmd.selector)
        break
      case 'screenshot': {
        const buf = await app.screenshot()
        output.write(`→ screenshot: ${buf.length} bytes captured\n`)
        break
      }
      case 'describe':
        // explicit describe is rendered by the auto-describe below; no-op here
        break
    }
  } catch (e) {
    err.write(formatError(e) + '\n')
    return { kind: 'dispatch-error' }
  }
  try {
    const screen = await app.describe()
    renderTop5(screen, output)
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    err.write(`→ describe failed: ${msg}\n`)
  }
  return { kind: 'action-success' }
}

function renderTop5(screen: ScreenDescription, output: WritableLike): void {
  const ts = new Date(screen.capturedAt).toISOString()
  const top = screen.elements.slice(0, 5)
  if (top.length === 0) {
    output.write(`→ screen: ${screen.frontApp} @ ${ts} (no visible elements)\n`)
    return
  }
  output.write(`→ screen: ${screen.frontApp} @ ${ts}\n`)
  top.forEach((el, i) => {
    output.write(`  ${i + 1}. ${formatElement(el)}\n`)
  })
}

function formatElement(el: ElementSummary): string {
  const name = el.name ?? '-'
  const id = el.id ?? '-'
  const b = el.bounds
  return `role=${el.role} name="${name}" id="${id}" bounds=(${b.x},${b.y},${b.w},${b.h})`
}

function formatError(e: unknown): string {
  if (e instanceof TapNotFoundError) {
    return `error: tap not found for selector ${JSON.stringify(e.selector)}; type 'describe' to see visible elements`
  }
  if (e instanceof RunnerTransportError) {
    return `error: runner transport: ${e.message}; check simctl + runner process is alive`
  }
  if (e instanceof ExpectationFailure) {
    return `error: ${e.code}: ${e.message}`
  }
  if (e instanceof Error) {
    return `error: ${e.name}: ${e.message}`
  }
  return `error: ${String(e)}`
}
