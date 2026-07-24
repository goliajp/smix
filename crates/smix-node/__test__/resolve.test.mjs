import { test } from 'node:test'
import assert from 'node:assert'
import { resolveSelector, resolveSelectorCount, resolveSelectorLabels } from '../index.js'

const TREE = JSON.stringify({
  rawType: 'application',
  enabled: true, selected: false, hasFocus: false, visible: true,
  bounds: { x: 0, y: 0, w: 0, h: 0 },
  children: [
    // role is what the resolver matches (the runner populates it via
    // derive_roles from rawType before serializing); a real tree carries it.
    { rawType: 'button', role: 'button', identifier: 'btn-ok', label: 'OK', enabled: true, selected: false, hasFocus: false, visible: true, bounds: { x: 0, y: 0, w: 10, h: 10 }, children: [] },
    { rawType: 'button', role: 'button', identifier: 'btn-cancel', label: 'Cancel', enabled: true, selected: false, hasFocus: false, visible: true, bounds: { x: 0, y: 0, w: 10, h: 10 }, children: [] },
  ],
})

test('resolveSelector returns the matching id (host-side, no wire)', () => {
  assert.deepStrictEqual(resolveSelector(TREE, '{"id":"btn-ok"}'), ['btn-ok'])
  assert.deepStrictEqual(resolveSelector(TREE, '{"id":"nope"}'), [])
})

test('resolveSelectorCount + labels', () => {
  assert.strictEqual(resolveSelectorCount(TREE, '{"role":"button"}'), 2)
  assert.deepStrictEqual(resolveSelectorLabels(TREE, '{"id":"btn-ok"}'), ['OK'])
})

test('invalid JSON throws a clear error', () => {
  assert.throws(() => resolveSelector('not json', '{}'), /invalid tree JSON/)
})
