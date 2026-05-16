import { describe, it, expect, vi } from 'vitest'
import {
  RunnerClient,
  TapNotFoundError,
  RunnerTransportError,
} from '../runner-client.js'
import type { A11yTreeSource } from '../a11y-tree-source.js'

function mkResponse(
  status: number,
  body: string | object = '',
  init: { ok?: boolean } = {},
): Response {
  const text = typeof body === 'string' ? body : JSON.stringify(body)
  return {
    ok: init.ok ?? (status >= 200 && status < 300),
    status,
    text: async () => text,
    json: async () => (typeof body === 'string' ? JSON.parse(text) : body),
  } as unknown as Response
}

describe('RunnerClient.health', () => {
  it('GET /health 200 returns true', async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(mkResponse(200, { ok: true }))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    expect(await client.health()).toBe(true)
    expect(fetchImpl).toHaveBeenCalledTimes(1)
    expect(fetchImpl.mock.calls[0]![0]).toBe('http://127.0.0.1:22087/health')
  })

  it('500 returns false', async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(mkResponse(500, 'boom', { ok: false }))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    expect(await client.health()).toBe(false)
  })

  it('fetch rejects returns false (does not throw)', async () => {
    const fetchImpl = vi.fn().mockRejectedValue(new Error('ECONNREFUSED'))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    expect(await client.health()).toBe(false)
  })
})

describe('RunnerClient.tap', () => {
  it('text happy: 200 resolves, URL + headers + body correct', async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(
        mkResponse(200, { ok: true, matched: { label: 'General' } }),
      )
    const client = new RunnerClient({ port: 22087, fetchImpl })
    await expect(client.tap({ text: 'General' })).resolves.toBeUndefined()
    expect(fetchImpl).toHaveBeenCalledTimes(1)
    const [url, init] = fetchImpl.mock.calls[0]!
    expect(url).toBe('http://127.0.0.1:22087/tap')
    expect((init as RequestInit).method).toBe('POST')
    const headers = (init as RequestInit).headers as Record<string, string>
    expect(headers['Content-Type']).toBe('application/json')
    expect((init as RequestInit).body).toBe(
      JSON.stringify({ selector: { text: 'General' } }),
    )
  })

  it('404 throws TapNotFoundError with selector + body', async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(mkResponse(404, '{"ok":false,"error":"not_found"}', { ok: false }))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    let caught: unknown
    try {
      await client.tap({ text: 'Nope' })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(TapNotFoundError)
    const err = caught as TapNotFoundError
    expect(err.selector).toEqual({ text: 'Nope' })
    expect(err.body).toContain('not_found')
  })

  it('5xx throws RunnerTransportError with body snippet', async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(mkResponse(500, 'internal boom', { ok: false }))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    let caught: unknown
    try {
      await client.tap({ text: 'X' })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(RunnerTransportError)
    const err = caught as RunnerTransportError
    expect(err.status).toBe(500)
    expect(err.message).toContain('500')
    expect(err.message).toContain('internal boom')
  })

  it('fetch reject throws RunnerTransportError wrapping cause', async () => {
    const cause = new Error('fetch failed: ECONNREFUSED')
    const fetchImpl = vi.fn().mockRejectedValue(cause)
    const client = new RunnerClient({ port: 22087, fetchImpl })
    let caught: unknown
    try {
      await client.tap({ text: 'X' })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(RunnerTransportError)
    const err = caught as RunnerTransportError
    expect(err.cause).toBe(cause)
    expect(err.message).toMatch(/fetch failed|ECONNREFUSED/)
  })

  it('custom port + host: URL reflects both', async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(mkResponse(200, { ok: true }))
    const client = new RunnerClient({
      port: 33000,
      host: 'localhost',
      fetchImpl,
    })
    await client.tap({ text: 'General' })
    expect(fetchImpl.mock.calls[0]![0]).toBe('http://localhost:33000/tap')
  })

  it('json parse failure on 404 still throws TapNotFoundError with raw body', async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(mkResponse(404, 'not json', { ok: false }))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    let caught: unknown
    try {
      await client.tap({ text: 'X' })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(TapNotFoundError)
    expect((caught as TapNotFoundError).body).toBe('not json')
  })
})

describe('RunnerClient.getTree', () => {
  // Minimal valid A11yNode payload — used as a building block in cases below.
  function leaf(overrides: Record<string, unknown> = {}): Record<string, unknown> {
    return {
      rawType: 'button',
      bounds: { x: 0, y: 0, w: 100, h: 44 },
      enabled: true,
      selected: false,
      hasFocus: false,
      visible: true,
      children: [],
      ...overrides,
    }
  }

  it('returns parsed A11yNode on 200', async () => {
    const body = leaf({
      rawType: 'application',
      identifier: 'com.apple.Preferences',
      bounds: { x: 0, y: 0, w: 393, h: 852 },
      children: [
        leaf({ rawType: 'cell', label: 'General', bounds: { x: 0, y: 100, w: 393, h: 44 } }),
      ],
    })
    const fetchImpl = vi.fn().mockResolvedValue(mkResponse(200, body))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    const result = await client.getTree()
    expect(result.rawType).toBe('application')
    expect(result.identifier).toBe('com.apple.Preferences')
    expect(result.children).toHaveLength(1)
    expect(result.children[0]!.rawType).toBe('cell')
    expect(result.children[0]!.label).toBe('General')
    // URL + method check.
    expect(fetchImpl).toHaveBeenCalledTimes(1)
    expect(fetchImpl.mock.calls[0]![0]).toBe('http://127.0.0.1:22087/tree')
  })

  it('throws RunnerTransportError on 500 (snapshot unavailable)', async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(
        mkResponse(500, '{"ok":false,"error":"snapshot_unavailable"}', { ok: false }),
      )
    const client = new RunnerClient({ port: 22087, fetchImpl })
    let caught: unknown
    try {
      await client.getTree()
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(RunnerTransportError)
    const err = caught as RunnerTransportError
    expect(err.status).toBe(500)
    expect(err.message).toContain('snapshot unavailable')
  })

  it('throws RunnerTransportError on malformed JSON (missing bounds)', async () => {
    // Payload is JSON-valid but fails A11yNode shape guard (no bounds).
    const malformed = {
      rawType: 'application',
      enabled: true,
      selected: false,
      hasFocus: false,
      visible: true,
      children: [],
    }
    const fetchImpl = vi.fn().mockResolvedValue(mkResponse(200, malformed))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    let caught: unknown
    try {
      await client.getTree()
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(RunnerTransportError)
    expect((caught as RunnerTransportError).message).toContain('malformed')
  })

  it('throws RunnerTransportError on fetch reject', async () => {
    const cause = new Error('ECONNREFUSED')
    const fetchImpl = vi.fn().mockRejectedValue(cause)
    const client = new RunnerClient({ port: 22087, fetchImpl })
    let caught: unknown
    try {
      await client.getTree()
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(RunnerTransportError)
    const err = caught as RunnerTransportError
    expect(err.cause).toBe(cause)
    expect(err.message).toMatch(/fetch failed|ECONNREFUSED/)
  })

  it('handles nested children two levels deep', async () => {
    const body = leaf({
      rawType: 'application',
      children: [
        leaf({
          rawType: 'window',
          children: [leaf({ rawType: 'cell', label: 'General' })],
        }),
      ],
    })
    const fetchImpl = vi.fn().mockResolvedValue(mkResponse(200, body))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    const result = await client.getTree()
    expect(result.children[0]!.children[0]!.rawType).toBe('cell')
    expect(result.children[0]!.children[0]!.label).toBe('General')
  })

  it('treats empty children as []', async () => {
    const body = leaf({ rawType: 'staticText', children: [] })
    const fetchImpl = vi.fn().mockResolvedValue(mkResponse(200, body))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    const result = await client.getTree()
    expect(result.children).toEqual([])
  })

  it('treats omitted identifier/label as undefined and does not throw', async () => {
    // No identifier or label keys — both optional in A11yNode.
    const body = leaf({ rawType: 'button' })
    const fetchImpl = vi.fn().mockResolvedValue(mkResponse(200, body))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    const result = await client.getTree()
    expect(result.identifier).toBeUndefined()
    expect(result.label).toBeUndefined()
  })

  it('throws RunnerTransportError on 4xx other than expected codes', async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(mkResponse(404, '<html>not found</html>', { ok: false }))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    let caught: unknown
    try {
      await client.getTree()
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(RunnerTransportError)
    expect((caught as RunnerTransportError).status).toBe(404)
  })
})

describe('RunnerClient.getTree role batch fill (v0.3 C3)', () => {
  function leaf(overrides: Record<string, unknown> = {}): Record<string, unknown> {
    return {
      rawType: 'button',
      bounds: { x: 0, y: 0, w: 100, h: 44 },
      enabled: true,
      selected: false,
      hasFocus: false,
      visible: true,
      children: [],
      ...overrides,
    }
  }

  it('root rawType=application → role omitted (not in KNOWN_ROLES)', async () => {
    const body = leaf({ rawType: 'application' })
    const fetchImpl = vi.fn().mockResolvedValue(mkResponse(200, body))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    const result = await client.getTree()
    expect(result.rawType).toBe('application')
    expect(result.role).toBeUndefined()
  })

  it('root rawType=button → role === "button" after batch fill', async () => {
    const body = leaf({ rawType: 'button' })
    const fetchImpl = vi.fn().mockResolvedValue(mkResponse(200, body))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    const result = await client.getTree()
    expect(result.role).toBe('button')
  })

  it('nested: window → button child gets role; window root role undefined', async () => {
    const body = leaf({
      rawType: 'window',
      children: [leaf({ rawType: 'button', label: 'OK' })],
    })
    const fetchImpl = vi.fn().mockResolvedValue(mkResponse(200, body))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    const result = await client.getTree()
    expect(result.role).toBeUndefined()
    expect(result.children[0]!.role).toBe('button')
  })

  it('3-level deep recursion: each rawType maps correctly', async () => {
    const body = leaf({
      rawType: 'button',
      children: [
        leaf({
          rawType: 'cell',
          children: [leaf({ rawType: 'staticText', label: 'Hello' })],
        }),
      ],
    })
    const fetchImpl = vi.fn().mockResolvedValue(mkResponse(200, body))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    const result = await client.getTree()
    expect(result.role).toBe('button')
    expect(result.children[0]!.role).toBe('cell')
    expect(result.children[0]!.children[0]!.role).toBe('staticText')
  })

  it('wire upstream role field is overwritten (always derived from rawType)', async () => {
    // Defensive: ensure no caller can sneak a stale/wrong role through the wire.
    // The post-processor always recomputes from rawType.
    const body = leaf({ rawType: 'button', role: 'wrongUpstreamRole' })
    const fetchImpl = vi.fn().mockResolvedValue(mkResponse(200, body))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    const result = await client.getTree()
    expect(result.role).toBe('button')
  })

  it('unknown rawType → role omitted (does not throw)', async () => {
    const body = leaf({ rawType: 'unknownFooBar' })
    const fetchImpl = vi.fn().mockResolvedValue(mkResponse(200, body))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    const result = await client.getTree()
    expect(result.rawType).toBe('unknownFooBar')
    expect(result.role).toBeUndefined()
  })

  it('Settings-like tree: application → window → cell containing button — descendants fill role', async () => {
    const body = leaf({
      rawType: 'application',
      identifier: 'com.apple.Preferences',
      bounds: { x: 0, y: 0, w: 393, h: 852 },
      children: [
        leaf({
          rawType: 'window',
          children: [
            leaf({
              rawType: 'cell',
              children: [leaf({ rawType: 'button', label: 'General' })],
            }),
          ],
        }),
      ],
    })
    const fetchImpl = vi.fn().mockResolvedValue(mkResponse(200, body))
    const client = new RunnerClient({ port: 22087, fetchImpl })
    const result = await client.getTree()
    expect(result.role).toBeUndefined()
    const window = result.children[0]!
    expect(window.role).toBeUndefined()
    const cell = window.children[0]!
    expect(cell.role).toBe('cell')
    expect(cell.children[0]!.role).toBe('button')
  })
})

describe('A11yTreeSource interface (v0.3 C3)', () => {
  it('RunnerClient structurally satisfies A11yTreeSource', () => {
    const client = new RunnerClient({ port: 22087 })
    // Structural typing: assignment compiles iff client.getTree() signature matches.
    const source: A11yTreeSource = client
    expect(typeof source.getTree).toBe('function')
  })

  it('A11yTreeSource.getTree returns a Promise', () => {
    const fakeNode = {
      rawType: 'button',
      bounds: { x: 0, y: 0, w: 1, h: 1 },
      enabled: true,
      selected: false,
      hasFocus: false,
      visible: true,
      children: [],
    }
    const fetchImpl = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => fakeNode,
      text: async () => JSON.stringify(fakeNode),
    } as unknown as Response)
    const source: A11yTreeSource = new RunnerClient({ port: 22087, fetchImpl })
    const p = source.getTree()
    expect(p).toBeInstanceOf(Promise)
    return p
  })
})
