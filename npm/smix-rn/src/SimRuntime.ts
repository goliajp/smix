// SmixSimRuntime interface + MockSimRuntime impl.
//
// Mirrors the Swift and Kotlin SimRuntime architecture. The TS SDK is
// HTTP-runner-friendly:
//   - HttpSimRuntime (real, lives in src/HttpRunner.ts): wraps the
//     smix-runner HTTP server via fetch (cross-platform iOS+Android).
//   - MockSimRuntime (here): in-memory tree + event log for unit tests.
//
// This module is intended for test targets only.

import type { A11yNode } from './A11yNode.js'

export type KeyName = 'return' | 'delete' | 'space' | 'tab' | 'escape' | 'enter'
export type SwipeDirection = 'up' | 'down' | 'left' | 'right'

/** Low-level emulator runtime — captures + injects events. */
export interface SmixSimRuntime {
  launch(bundleId: string): Promise<void>
  terminate(bundleId: string): Promise<void>
  snapshotTree(): Promise<A11yNode>
  synthesizeTap(x: number, y: number): Promise<void>
  sendString(text: string): Promise<void>
  pressKey(key: KeyName): Promise<void>
  swipe(direction: SwipeDirection): Promise<void>
  screenshot(): Promise<Uint8Array>
  systemPopups(): Promise<A11yNode[]>
  openUrl(url: string): Promise<void>
  launchFresh(opts: {
    bundleId: string
    clearState: boolean
    clearKeychain: boolean
    appPath?: string | undefined
  }): Promise<void>
  launchFromPath(appPath: string): Promise<void>
  synthesizeTapAtNormalized(nx: number, ny: number): Promise<void>
}

export interface LaunchFreshCall {
  bundleId: string
  clearState: boolean
  clearKeychain: boolean
  appPath: string | undefined
}

const EMPTY_DEFAULT: A11yNode = {
  rawType: 'other',
  bounds: { x: 0, y: 0, w: 393, h: 852 },
  enabled: true,
  visible: true,
  children: [],
}

/**
 * In-memory mock runtime — used by vitest unit tests for deterministic
 * wire verification without booting an emulator.
 */
export class MockSimRuntime implements SmixSimRuntime {
  snapshotResult: A11yNode = EMPTY_DEFAULT
  readonly tapCalls: { x: number; y: number }[] = []
  readonly launchCalls: string[] = []
  readonly terminateCalls: string[] = []
  readonly sendStringCalls: string[] = []
  readonly pressKeyCalls: KeyName[] = []
  readonly swipeCalls: SwipeDirection[] = []
  screenshotCalls = 0
  screenshotResult: Uint8Array = new Uint8Array(0)
  systemPopupsResult: A11yNode[] = []
  readonly openUrlCalls: string[] = []
  readonly launchFreshCalls: LaunchFreshCall[] = []
  readonly launchFromPathCalls: string[] = []
  readonly tapAtNormalizedCalls: { nx: number; ny: number }[] = []

  /** Hook invoked just before [snapshotTree] returns. Mirrors Swift/Kotlin. */
  afterSnapshot: ((callIndex: number) => void) | null = null
  private snapshotCallCount = 0

  constructor(opts: { snapshotResult?: A11yNode } = {}) {
    if (opts.snapshotResult !== undefined) this.snapshotResult = opts.snapshotResult
  }

  async launch(bundleId: string): Promise<void> {
    this.launchCalls.push(bundleId)
  }

  async terminate(bundleId: string): Promise<void> {
    this.terminateCalls.push(bundleId)
  }

  async snapshotTree(): Promise<A11yNode> {
    const idx = this.snapshotCallCount++
    this.afterSnapshot?.(idx)
    return this.snapshotResult
  }

  async synthesizeTap(x: number, y: number): Promise<void> {
    this.tapCalls.push({ x, y })
  }

  async sendString(text: string): Promise<void> {
    this.sendStringCalls.push(text)
  }

  async pressKey(key: KeyName): Promise<void> {
    this.pressKeyCalls.push(key)
  }

  async swipe(direction: SwipeDirection): Promise<void> {
    this.swipeCalls.push(direction)
  }

  async screenshot(): Promise<Uint8Array> {
    this.screenshotCalls++
    return this.screenshotResult
  }

  async systemPopups(): Promise<A11yNode[]> {
    return this.systemPopupsResult
  }

  async openUrl(url: string): Promise<void> {
    this.openUrlCalls.push(url)
  }

  async launchFresh(opts: {
    bundleId: string
    clearState: boolean
    clearKeychain: boolean
    appPath?: string | undefined
  }): Promise<void> {
    this.launchFreshCalls.push({
      bundleId: opts.bundleId,
      clearState: opts.clearState,
      clearKeychain: opts.clearKeychain,
      appPath: opts.appPath,
    })
  }

  async launchFromPath(appPath: string): Promise<void> {
    this.launchFromPathCalls.push(appPath)
  }

  async synthesizeTapAtNormalized(nx: number, ny: number): Promise<void> {
    this.tapAtNormalizedCalls.push({ nx, ny })
  }
}
