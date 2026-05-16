/**
 * Golden-path example: this is what an AI agent should write.
 *
 * Every line is a single observable side effect, every selector is
 * semantic (text / id / role+name), no coordinates, no sleeps.
 *
 * Run (once the driver is wired up):
 *   simx run examples/login.test.ts
 */
import { test, expect } from '../src/sdk/index.js'

test('login → onboarding → home', async ({ app }) => {
  await app.launch('com.example.app', { fresh: true })

  await app.tap({ text: 'Sign in' })
  await app.fill({ id: 'emailField' }, 'user@example.com')
  await app.fill({ id: 'passwordField' }, 'secret')
  await app.tap({ role: 'button', name: 'Continue' })

  await expect(app.element({ text: /Welcome/ })).toBeVisible()

  // Onboarding has multiple "Skip" buttons in different sections —
  // disambiguate by what's near them.
  await app.tap({ text: 'Skip', near: { text: 'Onboarding' } })

  await expect(app.element({ role: 'tab', name: 'Home' })).toBeVisible()
})

test('login rejects invalid credentials', async ({ app }) => {
  await app.launch('com.example.app', { fresh: true })
  await app.tap({ text: 'Sign in' })
  await app.fill({ id: 'emailField' }, 'nope@example.com')
  await app.fill({ id: 'passwordField' }, 'wrong')
  await app.tap({ role: 'button', name: 'Continue' })

  // Wait for the alert before asserting — the network call takes time.
  await app.waitFor({ role: 'alert' })
  await expect(app.element({ text: /Invalid/, inside: { role: 'alert' } }))
    .toBeVisible()
})
