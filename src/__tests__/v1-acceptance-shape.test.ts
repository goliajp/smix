// v0.7 C6 — shape-contract test for scripts/v1-acceptance.sh.
//
// We assert the 9-field single-line JSON schema literal (sc1_ai_0shot_assets,
// sc2_self_correct, sc3_cold_start_under_5s, sc4_tap_under_50ms,
// sc5_longrun_100, sc6_runtime_matrix, sc7_plugin_install, total, all_ok).
//
// We do NOT spawn v1-acceptance.sh from vitest because SC[4] performs a
// real cold xcodebuild test-runner spawn + host-hid digitizer tap probe,
// costing ~30-150s. That is verified end-to-end at the shell level via
// the Checkpoint C6 acceptance block. Here we lock down the JSON shape
// contract by (a) parsing the printf template from the script source and
// (b) round-tripping a representative stub through JSON.parse against
// the 9-key sort-stable signature. This mirrors the hot-plan decision
// 4.A.iii: shape invariant only, not SC[1-7] state.

import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

const SCRIPT_PATH = resolve(process.cwd(), 'scripts/v1-acceptance.sh')

const EXPECTED_KEYS_SORTED = [
  'all_ok',
  'sc1_ai_0shot_assets',
  'sc2_self_correct',
  'sc3_cold_start_under_5s',
  'sc4_tap_under_50ms',
  'sc5_longrun_100',
  'sc6_runtime_matrix',
  'sc7_plugin_install',
  'total',
] as const

// Representative stub matching v1-acceptance.sh's printf template (post-green).
const STUB_JSON =
  '{"sc1_ai_0shot_assets":"ok","sc2_self_correct":"ok","sc3_cold_start_under_5s":"ok","sc4_tap_under_50ms":"ok","sc5_longrun_100":"ok","sc6_runtime_matrix":"ok","sc7_plugin_install":"ok","total":7,"all_ok":"ok"}'

describe('v1-acceptance.sh shape contract (v0.7 C6)', () => {
  it('v1-acceptance.sh emits single-line valid JSON to stdout', () => {
    // Read script source and locate the printf template that emits the
    // stdout JSON. The template must (a) contain all 9 field keys, (b)
    // end with a single \n, (c) be a single-line literal (no embedded
    // unescaped newlines).
    const source = readFileSync(SCRIPT_PATH, 'utf-8')
    const printfMatch = source.match(
      /printf '\{[^']*"all_ok":"%s"\}\\n'/,
    )
    expect(printfMatch).not.toBeNull()
    const tmpl = printfMatch![0]
    // exactly one trailing \n marker
    expect(tmpl.match(/\\n/g)?.length).toBe(1)
    // single-line literal (no raw newline in template)
    expect(tmpl.includes('\n')).toBe(false)

    // The stub representative must round-trip through JSON.parse.
    expect(() => JSON.parse(STUB_JSON)).not.toThrow()
    expect(STUB_JSON.split('\n').length).toBe(1)
  })

  it('v1-acceptance.sh JSON has exactly 9 keys matching v1.md §4 7 SC + total + all_ok', () => {
    const json = JSON.parse(STUB_JSON) as Record<string, unknown>
    const keys = Object.keys(json).sort()
    expect(keys.length).toBe(9)
    expect(keys).toEqual([...EXPECTED_KEYS_SORTED])

    // Cross-check that every expected key appears in the script's printf
    // template literally — guards against drift between this test's stub
    // and v1-acceptance.sh emitted shape.
    const source = readFileSync(SCRIPT_PATH, 'utf-8')
    for (const key of EXPECTED_KEYS_SORTED) {
      expect(source).toContain(`"${key}"`)
    }
  })

  it('v1-acceptance.sh total field is 7 and all_ok field is ok|fail enum', () => {
    const json = JSON.parse(STUB_JSON) as { total: number; all_ok: string }
    expect(json.total).toBe(7)
    expect(['ok', 'fail']).toContain(json.all_ok)

    // Same constraints reflected in the script source: total is hard-coded
    // to the literal 7 (decision 2.A) and all_ok is set from $ALL_OK which
    // is set to one of {"ok","fail"} only.
    const source = readFileSync(SCRIPT_PATH, 'utf-8')
    expect(source).toMatch(/"total":%d.*\\n.*\n.*7\b/s)
    expect(source).toMatch(/ALL_OK="ok"/)
    expect(source).toMatch(/ALL_OK="fail"/)
  })
})
