import { describe, expect, it } from 'vitest'
import http from 'node:http'
import { App } from '../App.js'
import { loadNodeDriver } from '../loadNodeDriver.js'
import { MockSelectorResolver } from '../SelectorResolver.js'
import { Selector, encodeSelectorJson } from '../Selector.js'
import { Smix, bundleId } from '../Smix.js'

// Drives through the REAL @goliapkg/smix-node addon (resolved via the bun
// workspace) against a node:http loopback fake wire — no device, no publish.
// This is the seam's production leg: a real SmixNodeDriver satisfies NodeDriver
// and crosses napi to the loopback, the same shape C2 loopback-tested at the
// crate, now reached through smix-rn's loadNodeDriver factory.
const A11Y_ROOT =
  '{"rawType":"application","identifier":"root","enabled":true,"selected":false,"hasFocus":false,"visible":true,"bounds":{"x":0,"y":0,"w":0,"h":0}}'

interface Wire {
  port: number
  seen: Record<string, unknown>
  close: () => Promise<void>
}

async function startFakeWire(): Promise<Wire> {
  const seen: Record<string, unknown> = {}
  const server = http.createServer((req, res) => {
    let body = ''
    req.on('data', (c) => (body += c))
    req.on('end', () => {
      const parsed = body ? JSON.parse(body) : {}
      seen[req.url ?? ''] = parsed
      const reply =
        req.url === '/tree'
          ? A11Y_ROOT
          : req.url === '/session/open'
            ? '{"sessionId":"s-1"}'
            : '{"ok":true}'
      // `connection: close`, so the addon opens a fresh socket per
      // request rather than reusing this one.
      //
      // Measured on a 2-core Linux box (`taskset -c 0,1`), which is the
      // shape a CI runner has: a second request on a REUSED socket never
      // completes — not slowly, at all. Raised to a 60 s budget it sat
      // there for the whole 60 s. Same code under `bun` directly, and on
      // 16 cores, answers in milliseconds; at the previous tag the file
      // passed three times over because a tree read was one request, and
      // v11 made it two (the probe, then the tree).
      //
      // The weak side is this stand-in, not the wire: it is a node http
      // server living in the same vitest worker thread that is waiting
      // on the answer. The real runner is another process in another
      // language, and keep-alive there is worth having — a flow makes
      // hundreds of requests. So the stand-in stops pretending it can
      // hold a connection, and whether the client should pool at all is
      // recorded with its measurements rather than decided here.
      res.writeHead(200, { 'content-type': 'application/json', connection: 'close' })
      res.end(reply)
    })
  })
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()))
  const addr = server.address()
  const port = typeof addr === 'object' && addr ? addr.port : 0
  return { port, seen, close: () => new Promise((r) => server.close(() => r())) }
}

describe('smix-rn drives through the real napi addon (C4)', () => {
  it('loadNodeDriver resolves a real SmixNodeDriver satisfying the seam', async () => {
    const driver = await loadNodeDriver(1)
    expect(typeof driver.snapshotTree).toBe('function')
    expect(typeof driver.tapById).toBe('function')
    expect(typeof driver.openSession).toBe('function')
  })

  it('snapshotTree crosses the real .node to the loopback wire', async () => {
    const wire = await startFakeWire()
    try {
      const driver = await loadNodeDriver(wire.port)
      const answer = JSON.parse(await driver.snapshotTree())
      // Since v10 the tree arrives with the reader that produced it. The
      // two are not interchangeable: a screen the accessibility reader has
      // gone blind on and a screen with nothing on it are the same shape
      // without the source, which is why it travels with the tree rather
      // than beside it.
      expect(answer.source).toBe('a11y')
      expect(answer.root.identifier).toBe('root')
    } finally {
      await wire.close()
    }
  })

  it('Smix.launchApp with an explicit real driver launches and taps', async () => {
    const wire = await startFakeWire()
    try {
      const driver = await loadNodeDriver(wire.port)
      const resolver = new MockSelectorResolver()
      resolver.registerHit(encodeSelectorJson(Selector.id('root')), 'root')
      const app = await Smix.launchApp(bundleId('com.acme.app'), { driver, resolver: resolver.resolve })
      expect(app).toBeInstanceOf(App)

      await app.tap(Selector.id('root'))
      expect((wire.seen['/tap-by-id'] as { id?: string })?.id).toBe('root')
    } finally {
      await wire.close()
    }
  })
})
