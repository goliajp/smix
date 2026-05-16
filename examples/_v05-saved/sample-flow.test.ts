import { test } from '../../src/sdk/index.js'

// v0.5 C3 fixture — handcrafted proof that the REPL `.save as <name>`
// codegen path produces TS that compiles under tsconfig.examples.json.
// The `.save` output lands at examples/<name>.test.ts (one level deep)
// with `'../src/sdk/index.js'`; this file is the same content rehomed
// to `_v05-saved/` with the deeper relative import path.

test('sample-flow', async ({ app }) => {
  await app.launch('com.apple.Preferences')
})
