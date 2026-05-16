# examples/_v03-pending

Tests kept around as **spec / future golden path**, not currently
runnable.

- `login.test.ts` — the v1 SuccessCriteria [1] reference target.
  Needs `app.fill` (HID keyboard input). Resurrected by **v0.7+**
  (HID keyboard + runner `/type-text` endpoint).
  The v0.3 selector resolver portion is already showcased in
  `examples/login-tap.test.ts` (tap-only subset).
- `cart-checkout.test.ts` — needs `longPress` / `scroll` / `scrollTo`
  (v0.4 HID multi-action), `pasteboard.set` / `system.openUrl`
  (v0.5 system bridge). Resurrected by **v0.5**.

Both use `com.example.app` / `exampleshop://` placeholders that
don't exist on the host today; v0.7+ plan replaces with a real
sample app before merge.
