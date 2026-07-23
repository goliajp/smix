import { test } from 'node:test'
import assert from 'node:assert'
import http from 'node:http'

// The sense verbs: snapshotTree and systemPopups each return the wire's
// JSON as a string, the same convention as tapAtCoord. GET routes, so the
// mock replies by url alone.
function serving(replies) {
  return http.createServer((req, res) => {
    req.resume()
    req.on('end', () => {
      const body = replies[req.url]
      if (body === undefined) {
        res.writeHead(404).end()
        return
      }
      res.writeHead(200, { 'content-type': 'application/json' })
      res.end(body)
    })
  })
}

test('snapshotTree and systemPopups return the wire JSON', async () => {
  // A11yNode's required fields (no serde default): rawType, bounds, enabled,
  // selected, hasFocus, visible — the client deserializes the response, so
  // the mock must carry them all.
  const node = {
    rawType: 'application',
    identifier: 'root',
    bounds: { x: 0, y: 0, w: 0, h: 0 },
    enabled: true,
    selected: false,
    hasFocus: false,
    visible: true,
  }
  const server = serving({
    '/tree': JSON.stringify(node),
    '/system-popups': '{"popups":[{"id":"p1","type":"alert","source":"SpringBoard"}]}',
  })
  await new Promise((r) => server.listen(0, '127.0.0.1', r))
  const { port } = server.address()

  try {
    const { SmixNodeDriver } = await import('../index.js')
    const d = new SmixNodeDriver(port)

    const tree = JSON.parse(await d.snapshotTree())
    assert.strictEqual(tree.identifier, 'root')

    const popups = JSON.parse(await d.systemPopups())
    assert.strictEqual(popups[0].id, 'p1')
  } finally {
    await new Promise((r) => server.close(r))
  }
})
