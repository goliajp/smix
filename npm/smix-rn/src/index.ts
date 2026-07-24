// @goliapkg/smix public API exports.
//
// Mirrors Kotlin SDK + Swift SDK surface. This package is intended for
// test harnesses (vitest / jest / Detox-like runners) — it should not be
// bundled into a production RN app.

export {
  App,
} from './App.js'

export {
  MockNodeDriver,
  MockNodeSession,
  type NodeDriver,
  type NodeSession,
  type RecordedCall,
} from './NodeDriver.js'

export {
  type SystemPopup,
  type SystemPopupButton,
} from './SystemPopup.js'

export {
  defaultRunnerPort,
  loadNodeDriver,
  loadNodeResolver,
} from './loadNodeDriver.js'

export {
  literal,
  patternFromJson,
  patternToJson,
  regex,
  type Pattern,
} from './Pattern.js'

export {
  encodeSelectorJson,
  Selector,
  selectorFromJsonValue,
  selectorToJsonValue,
  type AnchorBox,
  type IndexModifiers,
  type Modifiers,
  type SelectorData,
  type SelectorKind,
} from './Selector.js'

export {
  ExpectationFailure,
  FAILURE_CODES,
  type FailureCode,
} from './ExpectationFailure.js'

export {
  Locator,
  SmixNotImplementedError,
} from './Locator.js'

export {
  appPath,
  bundleId,
  Smix,
  type AppTarget,
} from './Smix.js'

export {
  MockLabelsResolver,
  MockSelectorResolver,
  type LabelsResolver,
  type SelectorResolver,
} from './SelectorResolver.js'

export {
  HttpSimRuntime,
  type HttpFetch,
} from './HttpRunner.js'

export {
  findById,
  flatten,
  rectCenter,
  type A11yNode,
  type A11yRole,
  type Rect,
} from './A11yNode.js'

// v1.0.3 — session lifecycle. Wraps runner-side `/session/*` routes.
export {
  Session,
  type SessionOpenOptions,
} from './Session.js'
