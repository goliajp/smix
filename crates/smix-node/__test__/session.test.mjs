import { test } from 'node:test'
import assert from 'node:assert'
import http from 'node:http'

// The session lifecycle: openSession returns a SmixNodeSession bound to the
// runner's session id; launch/terminate/relaunch fire on it. The mock checks
// each request body so the session id is proven to thread through.
function routing(handlers) {
  return http.createServer((req, res) => {
    let body = ''
    req.on('data', (c) => (body += c))
    req.on('end', () => {
      const h = handlers[req.url]
      if (!h) {
        res.writeHead(404).end()
        return
      }
      const reply = h(body ? JSON.parse(body) : {})
      res.writeHead(200, { 'content-type': 'application/json' })
      res.end(JSON.stringify(reply))
    })
  })
}

test('openSession binds a session and lifecycle verbs thread its id', async () => {
  const seen = {}
  const server = routing({
    '/session/open': (b) => ((seen.open = b), { sessionId: 's-123' }),
    '/session/launch-app': (b) => ((seen.launch = b), { ok: true }),
    '/session/terminate-app': (b) => ((seen.terminate = b), { ok: true }),
    '/session/relaunch-app': (b) => ((seen.relaunch = b), { ok: true }),
  })
  await new Promise((r) => server.listen(0, '127.0.0.1', r))
  const { port } = server.address()

  try {
    const { SmixNodeDriver } = await import('../index.js')
    const d = new SmixNodeDriver(port)

    const s = await d.openSession('com.acme.app')
    assert.ok(s, 'openSession must resolve a session')
    assert.strictEqual(seen.open.bundleId, 'com.acme.app')

    await s.launchApp()
    assert.strictEqual(seen.launch.sessionId, 's-123')

    await s.terminateApp()
    assert.strictEqual(seen.terminate.sessionId, 's-123')

    await s.relaunchApp()
    assert.strictEqual(seen.relaunch.sessionId, 's-123')
  } finally {
    await new Promise((r) => server.close(r))
  }
})
