import type { Server } from '@modelcontextprotocol/sdk/server/index.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'
import type { Transport } from '@modelcontextprotocol/sdk/shared/transport.js'
import type { Logger } from './server.js'

export type CommandResult = { exitCode: number }

export type SignalKind = 'SIGINT' | 'SIGTERM'

export type SignalSubscribe = (cb: (sig: SignalKind) => void) => () => void

export type McpCommandDeps = {
  server: Server
  transport?: Transport
  log?: Logger
  onSignal?: SignalSubscribe
  stdin?: NodeJS.ReadableStream
}

const defaultLog: Logger = (msg) =>
  process.stderr.write(`[simx-mcp] ${msg}\n`)

function defaultSignalSubscribe(cb: (sig: SignalKind) => void): () => void {
  const onSigInt = (): void => cb('SIGINT')
  const onSigTerm = (): void => cb('SIGTERM')
  process.on('SIGINT', onSigInt)
  process.on('SIGTERM', onSigTerm)
  return () => {
    process.off('SIGINT', onSigInt)
    process.off('SIGTERM', onSigTerm)
  }
}

export async function runMcpCommand(deps: McpCommandDeps): Promise<CommandResult> {
  const log = deps.log ?? defaultLog
  const transport = deps.transport ?? new StdioServerTransport()

  try {
    await deps.server.connect(transport)
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    log(`server.connect failed: ${msg}`)
    return { exitCode: 1 }
  }
  log('server connected via stdio')

  const stdin: NodeJS.ReadableStream = deps.stdin ?? process.stdin
  const subscribe = deps.onSignal ?? defaultSignalSubscribe

  await new Promise<void>((resolve) => {
    let settled = false
    const finish = (reason: string): void => {
      if (settled) return
      settled = true
      log(reason)
      unsubscribe()
      stdin.removeListener('end', onEnd)
      resolve()
    }
    const unsubscribe = subscribe((sig) => {
      finish(`received ${sig}, shutting down`)
    })
    const onEnd = (): void => {
      finish('stdin EOF, shutting down')
    }
    stdin.once('end', onEnd)
  })

  try {
    await deps.server.close()
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    log(`server.close error (ignored): ${msg}`)
  }
  return { exitCode: 0 }
}
