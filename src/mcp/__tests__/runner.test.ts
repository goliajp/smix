import { EventEmitter } from 'node:events'
import { describe, it, expect, vi, afterEach } from 'vitest'
import type { Server } from '@modelcontextprotocol/sdk/server/index.js'
import type { Transport } from '@modelcontextprotocol/sdk/shared/transport.js'
import { runMcpCommand, type SignalKind } from '../runner.js'

function makeMockServer(overrides?: Partial<{ connect: () => Promise<void>; close: () => Promise<void> }>): Server {
  return {
    connect: vi.fn(overrides?.connect ?? (async () => undefined)),
    close: vi.fn(overrides?.close ?? (async () => undefined)),
  } as unknown as Server
}

function makeMockTransport(): Transport {
  return {} as Transport
}

function makeFakeStdin(): NodeJS.ReadableStream & { emitEnd: () => void } {
  const ee = new EventEmitter() as EventEmitter & {
    emitEnd: () => void
    once: EventEmitter['once']
    removeListener: EventEmitter['removeListener']
  }
  ;(ee as unknown as { emitEnd: () => void }).emitEnd = () => ee.emit('end')
  return ee as unknown as NodeJS.ReadableStream & { emitEnd: () => void }
}

type Subscriber = (sig: SignalKind) => void
function makeOnSignal(): {
  onSignal: (cb: Subscriber) => () => void
  trigger: (sig: SignalKind) => void
  unsubCount: () => number
} {
  const subs: Subscriber[] = []
  let unsubCount = 0
  return {
    onSignal: (cb) => {
      subs.push(cb)
      return () => {
        unsubCount += 1
      }
    },
    trigger: (sig) => {
      for (const cb of subs) cb(sig)
    },
    unsubCount: () => unsubCount,
  }
}

afterEach(() => {
  vi.restoreAllMocks()
})

describe('runMcpCommand', () => {
  it('happy: connects then resolves on stdin EOF, server.close called, exit 0', async () => {
    const server = makeMockServer()
    const transport = makeMockTransport()
    const stdin = makeFakeStdin()
    const log = vi.fn()
    const sig = makeOnSignal()
    const p = runMcpCommand({
      server,
      transport,
      stdin,
      onSignal: sig.onSignal,
      log,
    })
    queueMicrotask(() => stdin.emitEnd())
    const res = await p
    expect(server.connect).toHaveBeenCalledTimes(1)
    expect(server.connect).toHaveBeenCalledWith(transport)
    expect(server.close).toHaveBeenCalledTimes(1)
    expect(res.exitCode).toBe(0)
  })

  it('SIGINT: server.close called + exit 0', async () => {
    const server = makeMockServer()
    const transport = makeMockTransport()
    const stdin = makeFakeStdin()
    const sig = makeOnSignal()
    const log = vi.fn()
    const p = runMcpCommand({
      server,
      transport,
      stdin,
      onSignal: sig.onSignal,
      log,
    })
    queueMicrotask(() => sig.trigger('SIGINT'))
    const res = await p
    expect(server.close).toHaveBeenCalledTimes(1)
    expect(res.exitCode).toBe(0)
    const logged = log.mock.calls.map((c) => String(c[0])).join('\n')
    expect(logged).toContain('SIGINT')
  })

  it('SIGTERM: server.close called + exit 0', async () => {
    const server = makeMockServer()
    const stdin = makeFakeStdin()
    const sig = makeOnSignal()
    const log = vi.fn()
    const p = runMcpCommand({
      server,
      transport: makeMockTransport(),
      stdin,
      onSignal: sig.onSignal,
      log,
    })
    queueMicrotask(() => sig.trigger('SIGTERM'))
    const res = await p
    expect(server.close).toHaveBeenCalledTimes(1)
    expect(res.exitCode).toBe(0)
  })

  it('server.connect throws: log written + exit 1, no close', async () => {
    const server = makeMockServer({
      connect: async () => {
        throw new Error('bind failed: addr in use')
      },
    })
    const log = vi.fn()
    const res = await runMcpCommand({
      server,
      transport: makeMockTransport(),
      stdin: makeFakeStdin(),
      onSignal: () => () => undefined,
      log,
    })
    expect(res.exitCode).toBe(1)
    expect(server.close).not.toHaveBeenCalled()
    const logged = log.mock.calls.map((c) => String(c[0])).join('\n')
    expect(logged).toContain('bind failed')
  })

  it('server.close throws: log written + exit still 0 (graceful)', async () => {
    const server = makeMockServer({
      close: async () => {
        throw new Error('close kaboom')
      },
    })
    const stdin = makeFakeStdin()
    const sig = makeOnSignal()
    const log = vi.fn()
    const p = runMcpCommand({
      server,
      transport: makeMockTransport(),
      stdin,
      onSignal: sig.onSignal,
      log,
    })
    queueMicrotask(() => stdin.emitEnd())
    const res = await p
    expect(res.exitCode).toBe(0)
    const logged = log.mock.calls.map((c) => String(c[0])).join('\n')
    expect(logged).toContain('close kaboom')
  })

  it('default log writes to process.stderr (not stdout)', async () => {
    const server = makeMockServer()
    const stdin = makeFakeStdin()
    const sig = makeOnSignal()
    const errSpy = vi
      .spyOn(process.stderr, 'write')
      .mockImplementation(() => true)
    const outSpy = vi
      .spyOn(process.stdout, 'write')
      .mockImplementation(() => true)
    const p = runMcpCommand({
      server,
      transport: makeMockTransport(),
      stdin,
      onSignal: sig.onSignal,
    })
    queueMicrotask(() => stdin.emitEnd())
    await p
    // at least one call to stderr.write with the simx-mcp prefix
    const errCalls = errSpy.mock.calls.map((c) => String(c[0]))
    expect(errCalls.some((s) => s.includes('[simx-mcp]'))).toBe(true)
    const outCalls = outSpy.mock.calls.map((c) => String(c[0]))
    expect(outCalls.some((s) => s.includes('[simx-mcp]'))).toBe(false)
  })
})
