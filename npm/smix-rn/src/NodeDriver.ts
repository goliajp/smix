// The seam where live driving is supplied. The real driver is the napi
// addon (`@goliapkg/smix-node`, wired in with the prebuild matrix — the C4
// axis); its `SmixNodeDriver` / `SmixNodeSession` classes satisfy these
// interfaces structurally. The mocks answer from memory so the App's
// assembly logic (snapshot -> resolve -> tapById, range checks, enter ->
// return) is unit-tested without the native addon and without a device —
// the same seam-injection pattern as SelectorResolver.
//
// Every method returns what `crates/smix-node/index.d.ts` returns; the tree
// and popups cross as JSON strings, parsed on this side.

/** A live driver: the stateless act + sense verbs, plus session opening. */
export interface NodeDriver {
  tapById(id: string): Promise<boolean>
  inputText(text: string): Promise<void>
  pressKey(key: string): Promise<void>
  swipe(direction: string): Promise<void>
  tapAtCoord(nx: number, ny: number): Promise<string>
  swipeAtCoord(fromX: number, fromY: number, toX: number, toY: number): Promise<void>
  snapshotTree(): Promise<string>
  systemPopups(): Promise<string>
  openSession(bundleId: string): Promise<NodeSession>
}

/** A handle to an open session: the lifecycle verbs. */
export interface NodeSession {
  launchApp(): Promise<void>
  terminateApp(): Promise<void>
  relaunchApp(): Promise<void>
}

/** A recorded call: the verb name and the arguments it was given. */
export interface RecordedCall {
  readonly verb: string
  readonly args: readonly unknown[]
}

/**
 * Mock session for vitest. Records every lifecycle call; resolves them all.
 */
export class MockNodeSession implements NodeSession {
  readonly calls: RecordedCall[] = []

  async launchApp(): Promise<void> {
    this.calls.push({ verb: 'launchApp', args: [] })
  }

  async terminateApp(): Promise<void> {
    this.calls.push({ verb: 'terminateApp', args: [] })
  }

  async relaunchApp(): Promise<void> {
    this.calls.push({ verb: 'relaunchApp', args: [] })
  }
}

/**
 * Mock driver for vitest. Pre-set the tree / popups JSON it returns; records
 * every call; `tapById` reports a hit by default. `openSession` hands back a
 * fresh (or supplied) MockNodeSession so lifecycle calls are inspectable.
 */
export class MockNodeDriver implements NodeDriver {
  readonly calls: RecordedCall[] = []
  treeJson =
    '{"rawType":"application","enabled":true,"selected":false,"hasFocus":false,"visible":true,"bounds":{"x":0,"y":0,"w":0,"h":0}}'
  popupsJson = '[]'
  tapByIdResult = true
  readonly session: MockNodeSession

  constructor(session: MockNodeSession = new MockNodeSession()) {
    this.session = session
  }

  async tapById(id: string): Promise<boolean> {
    this.calls.push({ verb: 'tapById', args: [id] })
    return this.tapByIdResult
  }

  async inputText(text: string): Promise<void> {
    this.calls.push({ verb: 'inputText', args: [text] })
  }

  async pressKey(key: string): Promise<void> {
    this.calls.push({ verb: 'pressKey', args: [key] })
  }

  async swipe(direction: string): Promise<void> {
    this.calls.push({ verb: 'swipe', args: [direction] })
  }

  async tapAtCoord(nx: number, ny: number): Promise<string> {
    this.calls.push({ verb: 'tapAtCoord', args: [nx, ny] })
    return '{"chain":[]}'
  }

  async swipeAtCoord(fromX: number, fromY: number, toX: number, toY: number): Promise<void> {
    this.calls.push({ verb: 'swipeAtCoord', args: [fromX, fromY, toX, toY] })
  }

  async snapshotTree(): Promise<string> {
    this.calls.push({ verb: 'snapshotTree', args: [] })
    return this.treeJson
  }

  async systemPopups(): Promise<string> {
    this.calls.push({ verb: 'systemPopups', args: [] })
    return this.popupsJson
  }

  async openSession(bundleId: string): Promise<NodeSession> {
    this.calls.push({ verb: 'openSession', args: [bundleId] })
    return this.session
  }
}
