import { test } from '../src/sdk/index.js'

test('screenshot only', async ({ app }) => {
  await app.launch('com.apple.mobilesafari')
  const png = await app.screenshot()
  if (png.length < 1000) {
    throw new Error(`screenshot too small: ${png.length} bytes`)
  }
  if (!(png[0] === 0x89 && png[1] === 0x50 && png[2] === 0x4e && png[3] === 0x47)) {
    throw new Error('screenshot missing PNG magic bytes')
  }
})
