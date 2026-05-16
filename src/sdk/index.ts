export { App } from './app.js'
export { ElementHandle } from './element.js'
export { expect } from './expect.js'
export { ElementExpectation, ScreenExpectation } from './matchers.js'
export { test, run, listTests, resetRegistry } from './test.js'
export type { TestContext, TestFn, RunOptions, RunResult } from './test.js'

// Re-export core types so test authors only need one import.
export type {
  Selector,
  Role,
  A11yNode,
  ElementSummary,
  ScreenDescription,
  Rect,
  FailureCode,
} from '../core/index.js'
export { ExpectationFailure, describeSelector } from '../core/index.js'

export type {
  SwipeDirection,
  KeyName,
  Permission,
  NetworkState,
  Appearance,
  LaunchOptions,
} from '../driver/index.js'
