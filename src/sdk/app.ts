import type {
  Driver,
  SwipeDirection,
  KeyName,
  Permission,
  NetworkState,
  Appearance,
  LaunchOptions,
} from '../driver/index.js'
import type { Selector, ScreenDescription } from '../core/index.js'
import { ElementHandle } from './element.js'

/**
 * Top-level surface a test author calls. Mirrors Playwright's `page`
 * deliberately: AI authoring quality is highest when names overlap
 * with the corpus it trained on.
 *
 * Every method is async — no chaining shortcuts, no fluent builder
 * pattern. One step, one await, one observable side effect.
 */
export class App {
  constructor(readonly driver: Driver) {}

  // -- App lifecycle --
  launch(bundleId: string, opts?: LaunchOptions): Promise<void> {
    return this.driver.launch(bundleId, opts)
  }
  terminate(bundleId: string): Promise<void> {
    return this.driver.terminate(bundleId)
  }
  install(pathToApp: string): Promise<void> {
    return this.driver.install(pathToApp)
  }
  uninstall(bundleId: string): Promise<void> {
    return this.driver.uninstall(bundleId)
  }
  background(): Promise<void> {
    return this.driver.background()
  }
  foreground(bundleId: string): Promise<void> {
    return this.driver.foreground(bundleId)
  }

  // -- Interaction --
  tap(selector: Selector): Promise<void> {
    return this.driver.tap(selector)
  }
  doubleTap(selector: Selector): Promise<void> {
    return this.driver.doubleTap(selector)
  }
  longPress(selector: Selector, opts?: { durationMs?: number }): Promise<void> {
    return this.driver.longPress(selector, opts?.durationMs)
  }
  fill(selector: Selector, text: string): Promise<void> {
    return this.driver.fill(selector, text)
  }
  clear(selector: Selector): Promise<void> {
    return this.driver.clear(selector)
  }
  swipe(direction: SwipeDirection, opts?: { from?: Selector }): Promise<void> {
    return this.driver.swipe(direction, opts?.from)
  }
  scroll(selector: Selector, direction: 'up' | 'down'): Promise<void> {
    return this.driver.scroll(selector, direction)
  }
  scrollTo(selector: Selector): Promise<void> {
    return this.driver.scrollTo(selector)
  }

  // -- Keyboard --
  pressKey(key: KeyName): Promise<void> {
    return this.driver.pressKey(key)
  }
  hideKeyboard(): Promise<void> {
    return this.driver.hideKeyboard()
  }

  // -- Waiting --
  waitFor(selector: Selector, opts?: { timeoutMs?: number }): Promise<void> {
    return this.driver.waitFor(selector, opts?.timeoutMs).then(() => undefined)
  }

  // -- Capture --
  screenshot(): Promise<Buffer> {
    return this.driver.screenshot()
  }
  describe(): Promise<ScreenDescription> {
    return this.driver.describe()
  }

  // -- Queries --
  element(selector: Selector): ElementHandle {
    return new ElementHandle(this.driver, selector)
  }

  // -- Grouped surfaces (Playwright shape: `app.pasteboard.set(...)`) --
  readonly pasteboard = {
    set: (text: string) => this.driver.pasteboardSet(text),
    get: () => this.driver.pasteboardGet(),
  }

  readonly permissions = {
    grant: (permission: Permission, bundleId?: string) =>
      this.driver.grantPermission(permission, bundleId),
  }

  readonly system = {
    openUrl: (url: string) => this.driver.openUrl(url),
    setAppearance: (mode: Appearance) => this.driver.setAppearance(mode),
    setLocale: (locale: string) => this.driver.setLocale(locale),
    setNetwork: (state: NetworkState) => this.driver.setNetwork(state),
  }
}
