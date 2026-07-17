// HttpSimRuntime fetch tests over an injected mock client.
//
// These pass against a mock and say nothing about the real runner: the
// routes asserted here are the ones this SDK sends, not the ones the
// runner serves. See the defect noted at the top of HttpRunner.ts.
//
// Verifies:
//   - resolver POSTs to /select/resolve with treeJson + selectorJson + decodes ids
//   - labelsResolver POSTs to /select/resolve-labels + decodes labels
//   - resolveCount POSTs to /select/resolve-count + decodes count
//   - Wire body shape matches Rust `{treeJson, selectorJson}` contract
//   - Errors propagate cleanly

import { describe, expect, test } from 'vitest'
import { HttpSimRuntime, type HttpFetch } from '../HttpRunner.js'

type CapturedRequest = {
  url: string
  body: string
}

/** Build a mock fetch that captures requests and returns a stub JSON body. */
function mockFetch(stub: unknown, captured: CapturedRequest[]): HttpFetch {
  return async (url, init) => {
    captured.push({ url, body: init.body })
    return {
      ok: true,
      status: 200,
      json: async () => stub,
      text: async () => JSON.stringify(stub),
    }
  }
}

function failingFetch(): HttpFetch {
  return async (url) => ({
    ok: false,
    status: 500,
    json: async () => ({}),
    text: async () => `server error at ${url}`,
  })
}

describe('HttpSimRuntime.resolver', () => {
  test('POSTs to /select/resolve with treeJson + selectorJson', async () => {
    const captured: CapturedRequest[] = []
    const runtime = new HttpSimRuntime('http://localhost:14101', mockFetch({ ids: ['btn-a', 'btn-b'] }, captured))

    const ids = await runtime.resolver('{"rawType":"other"}', '{"id":"btn"}')

    expect(ids).toEqual(['btn-a', 'btn-b'])
    expect(captured.length).toBe(1)
    expect(captured[0]?.url).toBe('http://localhost:14101/select/resolve')
    expect(JSON.parse(captured[0]?.body ?? '')).toEqual({
      treeJson: '{"rawType":"other"}',
      selectorJson: '{"id":"btn"}',
    })
  })

  test('throws on 500', async () => {
    const runtime = new HttpSimRuntime('http://localhost:14101', failingFetch())
    await expect(runtime.resolver('{}', '{}')).rejects.toMatchObject({
      message: expect.stringContaining('HTTP 500'),
    })
  })
})

describe('HttpSimRuntime.labelsResolver (v7.7 c1)', () => {
  test('POSTs to /select/resolve-labels with treeJson + selectorJson', async () => {
    const captured: CapturedRequest[] = []
    const runtime = new HttpSimRuntime(
      'http://localhost:14101',
      mockFetch({ labels: ['Item 1', 'Item 2', 'Item 3'] }, captured),
    )

    const labels = await runtime.labelsResolver('{"rawType":"other"}', '{"label":"Item"}')

    expect(labels).toEqual(['Item 1', 'Item 2', 'Item 3'])
    expect(captured[0]?.url).toBe('http://localhost:14101/select/resolve-labels')
    expect(JSON.parse(captured[0]?.body ?? '')).toEqual({
      treeJson: '{"rawType":"other"}',
      selectorJson: '{"label":"Item"}',
    })
  })
})

describe('HttpSimRuntime.resolveCount (v7.7 c1)', () => {
  test('POSTs to /select/resolve-count and decodes count', async () => {
    const captured: CapturedRequest[] = []
    const runtime = new HttpSimRuntime(
      'http://localhost:14101',
      mockFetch({ count: 7 }, captured),
    )

    const n = await runtime.resolveCount('{}', '{"role":"button"}')

    expect(n).toBe(7)
    expect(captured[0]?.url).toBe('http://localhost:14101/select/resolve-count')
  })
})

describe('HttpSimRuntime runtime methods wire shape', () => {
  test('synthesizeTap → POST /input/tap with {x,y}', async () => {
    const captured: CapturedRequest[] = []
    const runtime = new HttpSimRuntime('http://localhost:14101', mockFetch({}, captured))
    await runtime.synthesizeTap(140.5, 220.0)
    expect(captured[0]?.url).toBe('http://localhost:14101/input/tap')
    expect(JSON.parse(captured[0]?.body ?? '')).toEqual({ x: 140.5, y: 220.0 })
  })

  test('launch → POST /sim/launch with {bundleId}', async () => {
    const captured: CapturedRequest[] = []
    const runtime = new HttpSimRuntime('http://localhost:14101', mockFetch({}, captured))
    await runtime.launch('dev.smix.MyApp')
    expect(captured[0]?.url).toBe('http://localhost:14101/sim/launch')
    expect(JSON.parse(captured[0]?.body ?? '')).toEqual({ bundleId: 'dev.smix.MyApp' })
  })

  test('synthesizeTapAtNormalized → POST /input/tap-normalized', async () => {
    const captured: CapturedRequest[] = []
    const runtime = new HttpSimRuntime('http://localhost:14101', mockFetch({}, captured))
    await runtime.synthesizeTapAtNormalized(0.5, 0.5)
    expect(captured[0]?.url).toBe('http://localhost:14101/input/tap-normalized')
  })
})
