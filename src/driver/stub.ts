import type { Driver } from './types.js'
import { ExpectationFailure } from '../core/index.js'

/**
 * v0 placeholder driver. Throws on every call so accidentally running
 * a test before the Swift bridge is wired up surfaces a clear error
 * instead of silent no-ops.
 */
export class StubDriver implements Driver {
  // -- App lifecycle --
  launch(): Promise<void> { return this.unimplemented('launch') }
  terminate(): Promise<void> { return this.unimplemented('terminate') }
  install(): Promise<void> { return this.unimplemented('install') }
  uninstall(): Promise<void> { return this.unimplemented('uninstall') }
  background(): Promise<void> { return this.unimplemented('background') }
  foreground(): Promise<void> { return this.unimplemented('foreground') }

  // -- Interaction --
  tap(): Promise<void> { return this.unimplemented('tap') }
  doubleTap(): Promise<void> { return this.unimplemented('doubleTap') }
  longPress(): Promise<void> { return this.unimplemented('longPress') }
  fill(): Promise<void> { return this.unimplemented('fill') }
  clear(): Promise<void> { return this.unimplemented('clear') }
  swipe(): Promise<void> { return this.unimplemented('swipe') }
  scroll(): Promise<void> { return this.unimplemented('scroll') }
  scrollTo(): Promise<void> { return this.unimplemented('scrollTo') }

  // -- Keyboard --
  pressKey(): Promise<void> { return this.unimplemented('pressKey') }
  hideKeyboard(): Promise<void> { return this.unimplemented('hideKeyboard') }

  // -- System --
  openUrl(): Promise<void> { return this.unimplemented('openUrl') }
  pasteboardSet(): Promise<void> { return this.unimplemented('pasteboardSet') }
  pasteboardGet(): Promise<string> { return this.unimplemented('pasteboardGet') }
  grantPermission(): Promise<void> { return this.unimplemented('grantPermission') }
  setAppearance(): Promise<void> { return this.unimplemented('setAppearance') }
  setLocale(): Promise<void> { return this.unimplemented('setLocale') }
  setNetwork(): Promise<void> { return this.unimplemented('setNetwork') }

  // -- Waiting / query --
  waitFor(): Promise<never> { return this.unimplemented('waitFor') }
  screenshot(): Promise<Buffer> { return this.unimplemented('screenshot') }
  tree(): Promise<never> { return this.unimplemented('tree') }
  describe(): Promise<never> { return this.unimplemented('describe') }
  findOne(): Promise<never> { return this.unimplemented('findOne') }
  findAll(): Promise<never> { return this.unimplemented('findAll') }

  private unimplemented(op: string): Promise<never> {
    return Promise.reject(
      new ExpectationFailure({
        code: 'DRIVER_ERROR',
        message: `Driver.${op}() not implemented in v0 stub. ` +
          `Wire the simulator bridge in src/driver/ before running tests.`,
        hint: 'See docs/design.md → "v0 (4 weeks)" for the driver work plan.',
      })
    )
  }
}
