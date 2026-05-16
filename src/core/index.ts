export type { Role, XCUIElementTypeName, XCUIElementTypeValue } from './role.js'
export { XCUIElementType, roleToElementTypes, elementTypeToRole } from './role.js'

export type {
  Selector,
  BaseText,
  BaseId,
  BaseLabel,
  BaseRole,
  Modifiers,
} from './selector.js'
export {
  isTextSelector,
  isIdSelector,
  isLabelSelector,
  isRoleSelector,
  describeSelector,
} from './selector.js'

export type { Rect, A11yNode, ElementSummary, ScreenDescription } from './screen.js'
export { summarizeNode } from './screen.js'

export type { FailureCode, FailureInit, SerializedFailure } from './error.js'
export { ExpectationFailure } from './error.js'

export { resolveSelector, resolveSelectorAll } from './resolve-selector.js'

export {
  rectSchema,
  roleSchema,
  elementSummarySchema,
  a11yNodeSchema,
  screenDescriptionSchema,
  failureCodeSchema,
  serializedFailureSchema,
} from './schemas.js'
