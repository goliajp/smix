import type { Selector, A11yNode, ScreenDescription } from '../core/index.js'

export type SwipeDirection = 'up' | 'down' | 'left' | 'right'

export type KeyName =
  | 'return'
  | 'delete'
  | 'tab'
  | 'space'
  | 'escape'
  | 'arrowUp'
  | 'arrowDown'
  | 'arrowLeft'
  | 'arrowRight'

export type Permission =
  | 'camera'
  | 'photos'
  | 'location'
  | 'locationAlways'
  | 'notifications'
  | 'microphone'
  | 'contacts'
  | 'calendar'
  | 'reminders'
  | 'bluetooth'
  | 'motion'
  | 'faceId'

export type NetworkState = 'online' | 'offline' | 'slow-3g'
export type Appearance = 'light' | 'dark'

export type LaunchOptions = {
  args?: string[]
  env?: Record<string, string>
  /** if true, terminate any running instance before launch */
  fresh?: boolean
}

export type FindOptions = {
  /** milliseconds; 0 means single-shot query */
  timeout?: number
}

/**
 * Platform-agnostic interface implemented by the simulator bridge.
 * SDK and MCP both consume this. The default v0 impl is a stub
 * that throws — real implementations live in src/driver/ios26.ts etc.
 */
export interface Driver {
  // -- App lifecycle --
  launch(bundleId: string, opts?: LaunchOptions): Promise<void>
  terminate(bundleId: string): Promise<void>
  install(path: string): Promise<void>
  uninstall(bundleId: string): Promise<void>
  background(): Promise<void>
  foreground(bundleId: string): Promise<void>

  // -- Interaction --
  tap(selector: Selector): Promise<void>
  doubleTap(selector: Selector): Promise<void>
  longPress(selector: Selector, durationMs?: number): Promise<void>
  fill(selector: Selector, text: string): Promise<void>
  clear(selector: Selector): Promise<void>
  swipe(direction: SwipeDirection, from?: Selector): Promise<void>
  scroll(selector: Selector, direction: 'up' | 'down'): Promise<void>
  scrollTo(selector: Selector): Promise<void>

  // -- Keyboard --
  pressKey(key: KeyName): Promise<void>
  hideKeyboard(): Promise<void>

  // -- System --
  openUrl(url: string): Promise<void>
  pasteboardSet(text: string): Promise<void>
  pasteboardGet(): Promise<string>
  grantPermission(permission: Permission, bundleId?: string): Promise<void>
  setAppearance(mode: Appearance): Promise<void>
  setLocale(locale: string): Promise<void>
  setNetwork(state: NetworkState): Promise<void>

  // -- Waiting --
  waitFor(selector: Selector, timeoutMs?: number): Promise<A11yNode>

  // -- Capture / query --
  screenshot(): Promise<Buffer>
  tree(): Promise<A11yNode>
  describe(): Promise<ScreenDescription>
  findOne(selector: Selector, opts?: FindOptions): Promise<A11yNode | null>
  findAll(selector: Selector): Promise<A11yNode[]>
}
