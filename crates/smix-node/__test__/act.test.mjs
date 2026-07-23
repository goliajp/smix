import { test } from 'node:test'
import assert from 'node:assert'
import http from 'node:http'

// The stateless act verbs, each crossing the async bridge to a loopback
// wire: tapById returns the runner's ok bool; inputText/pressKey/swipe
// fire and resolve. The mock also checks the request body so the boundary
// is proven to pass the argument through, not just to resolve.
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
      const parsed = body ? JSON.parse(body) : {}
      const reply = h(parsed)
      res.writeHead(200, { 'content-type': 'application/json' })
      res.end(JSON.stringify(reply))
    })
  })
}

test('the stateless act verbs cross the bridge and pass their args', async () => {
  const seen = {}
  const server = routing({
    '/tap-by-id': (b) => ((seen.tap = b), { ok: true }),
    '/input-text': (b) => ((seen.input = b), { ok: true }),
    '/press-key': (b) => ((seen.key = b), {}),
    '/swipe-once': (b) => ((seen.swipe = b), { ok: true }),
  })
  await new Promise((r) => server.listen(0, '127.0.0.1', r))
  const { port } = server.address()

  try {
    const { SmixNodeDriver } = await import('../index.js')
    const d = new SmixNodeDriver(port)

    const ok = await d.tapById('btn-ok')
    assert.strictEqual(ok, true)
    assert.strictEqual(seen.tap.id, 'btn-ok')

    await d.inputText('hello')
    assert.strictEqual(seen.input.text, 'hello')

    await d.pressKey('return')
    assert.strictEqual(seen.key.key, 'return')

    await d.swipe('up')
    assert.strictEqual(seen.swipe.direction, 'up')
  } finally {
    await new Promise((r) => server.close(r))
  }
})
