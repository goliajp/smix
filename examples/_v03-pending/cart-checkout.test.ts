/**
 * Demonstrates role+name selectors, relative positioning, scrolling
 * into view, and system-level helpers (pasteboard, deep link).
 */
import { test, expect } from '../src/sdk/index.js'

test('add to cart and checkout via deep link', async ({ app }) => {
  // Open the product page directly instead of navigating through Home.
  await app.system.openUrl('exampleshop://product/sku-123')

  await expect(app.element({ text: 'Wireless headphones' })).toBeVisible()
  await app.tap({ role: 'button', name: 'Add to cart' })

  await app.tap({ role: 'tab', name: 'Cart' })
  await expect(app.element({ role: 'cell', name: /Wireless headphones/ }))
    .toHaveCount(1)

  // The "Checkout" button is below the totals row — scroll if offscreen.
  await app.scrollTo({ role: 'button', name: 'Checkout' })
  await app.tap({ role: 'button', name: 'Checkout' })

  // Paste a saved promo instead of typing.
  await app.pasteboard.set('SUMMER25')
  await app.longPress({ id: 'promoCodeField' })
  await app.tap({ text: 'Paste' })

  await expect(app.element({ text: /25% off/ })).toBeVisible()
})
