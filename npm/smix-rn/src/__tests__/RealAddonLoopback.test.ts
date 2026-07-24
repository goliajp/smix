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
      res.writeHead(200, { 'content-type': 'application/json' })
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
      const tree = JSON.parse(await driver.snapshotTree())
      expect(tree.identifier).toBe('root')
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
