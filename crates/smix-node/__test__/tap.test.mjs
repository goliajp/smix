import { test } from 'node:test'
import assert from 'node:assert'
import http from 'node:http'

// The bridge gate: a real tapAtCoord crosses napi's tokio async bridge,
// reaches a wire endpoint over HTTP, and the Promise resolves with the
// structured hit chain — no device, the endpoint is a local loopback mock
// standing in for the runner. A future with no reactor would panic here
// (the failure the driving layer warns about); a resolved Promise proves
// the bridge is wired.
test('tapAtCoord crosses the async bridge and resolves the hit chain', async () => {
  const server = http.createServer((req, res) => {
    let body = ''
    req.on('data', (c) => (body += c))
    req.on('end', () => {
      assert.strictEqual(req.method, 'POST')
      assert.strictEqual(req.url, '/tap-at-norm-coord')
      res.writeHead(200, { 'content-type': 'application/json' })
      // frame is a required HitChainEntry field (smix-runner-wire); ok is
      // read by the client's require_ok envelope.
      res.end(
        JSON.stringify({
          ok: true,
          chain: [{ identifier: 'btn-ok', frame: { x: 0, y: 0, w: 10, h: 10 } }],
        }),
      )
    })
  })
  await new Promise((r) => server.listen(0, '127.0.0.1', r))
  const { port } = server.address()

  try {
    const { SmixNodeDriver } = await import('../index.js')
    const d = new SmixNodeDriver(port)
    const raw = await d.tapAtCoord(0.5, 0.5)
    const parsed = JSON.parse(raw)
    assert.strictEqual(parsed.chain[0].identifier, 'btn-ok')
  } finally {
    await new Promise((r) => server.close(r))
  }
})
