/**
 * Semantic roles modeled on Playwright's getByRole, mapped to iOS
 * XCUIElement.ElementType under the hood.
 *
 * Surface is Playwright-shaped to maximize AI authoring quality
 * (training corpus matches). Translation to XCUIElement.ElementType
 * is an engineering detail handled by the driver layer.
 */
import type { Role } from './schemas.js'

export type { Role } from './schemas.js'

/**
 * XCUIElement.ElementType numeric constants Apple ships.
 * Source: XCTest/XCUIElementTypes.h
 */
export const XCUIElementType = {
  any: 0,
  other: 1,
  application: 2,
  group: 3,
  window: 4,
  sheet: 5,
  drawer: 6,
  alert: 7,
  dialog: 8,
  button: 9,
  radioButton: 10,
  radioGroup: 11,
  checkBox: 12,
  disclosureTriangle: 13,
  popUpButton: 14,
  comboBox: 15,
  menuButton: 16,
  toolbarButton: 17,
  popover: 18,
  keyboard: 19,
  key: 20,
  navigationBar: 21,
  tabBar: 22,
  tabGroup: 23,
  toolbar: 24,
  statusBar: 25,
  table: 26,
  tableRow: 27,
  tableColumn: 28,
  outline: 29,
  outlineRow: 30,
  browser: 31,
  collectionView: 32,
  slider: 33,
  pageIndicator: 34,
  progressIndicator: 35,
  activityIndicator: 36,
  segmentedControl: 37,
  picker: 38,
  pickerWheel: 39,
  switch: 40,
  toggle: 41,
  link: 42,
  image: 43,
  icon: 44,
  searchField: 45,
  scrollView: 46,
  scrollBar: 47,
  staticText: 48,
  textField: 49,
  secureTextField: 50,
  datePicker: 51,
  textView: 52,
  menu: 53,
  menuItem: 54,
  menuBar: 55,
  menuBarItem: 56,
  map: 57,
  webView: 58,
  cell: 75,
} as const

export type XCUIElementTypeName = keyof typeof XCUIElementType
export type XCUIElementTypeValue = (typeof XCUIElementType)[XCUIElementTypeName]

/**
 * Map a Playwright-style Role to one or more XCUIElement.ElementType ids.
 * Returning multiple ids covers cases where a single semantic role
 * corresponds to several iOS element types (e.g. switch & toggle).
 */
export function roleToElementTypes(role: Role): readonly XCUIElementTypeValue[] {
  switch (role) {
    case 'button':
      return [XCUIElementType.button, XCUIElementType.menuButton, XCUIElementType.toolbarButton ?? XCUIElementType.button]
    case 'link':
      return [XCUIElementType.link]
    case 'textField':
      return [XCUIElementType.textField, XCUIElementType.textView]
    case 'secureTextField':
      return [XCUIElementType.secureTextField]
    case 'searchField':
      return [XCUIElementType.searchField]
    case 'switch':
    case 'toggle':
      return [XCUIElementType.switch, XCUIElementType.toggle]
    case 'checkBox':
      return [XCUIElementType.checkBox]
    case 'radio':
      return [XCUIElementType.radioButton]
    case 'image':
      return [XCUIElementType.image, XCUIElementType.icon]
    case 'staticText':
      return [XCUIElementType.staticText]
    case 'tab':
      return [XCUIElementType.tabBar, XCUIElementType.tabGroup]
    case 'tabBar':
      return [XCUIElementType.tabBar]
    case 'navigationBar':
      return [XCUIElementType.navigationBar]
    case 'cell':
      return [XCUIElementType.cell, XCUIElementType.tableRow]
    case 'alert':
    case 'dialog':
      return [XCUIElementType.alert, XCUIElementType.dialog, XCUIElementType.sheet]
    case 'slider':
      return [XCUIElementType.slider]
    case 'progressBar':
      return [XCUIElementType.progressIndicator, XCUIElementType.activityIndicator]
    case 'picker':
      return [XCUIElementType.picker, XCUIElementType.pickerWheel, XCUIElementType.datePicker]
    case 'menu':
      return [XCUIElementType.menu, XCUIElementType.menuBar]
    case 'menuItem':
      return [XCUIElementType.menuItem, XCUIElementType.menuBarItem]
    case 'scrollView':
      return [XCUIElementType.scrollView]
    case 'segmentedControl':
      return [XCUIElementType.segmentedControl]
    case 'table':
      return [XCUIElementType.table]
    case 'collectionView':
      return [XCUIElementType.collectionView]
    case 'webView':
      return [XCUIElementType.webView]
    case 'keyboard':
      return [XCUIElementType.keyboard]
  }
}

/**
 * Reverse mapping for surfacing roles in a11y tree dumps and
 * AI-facing error messages.
 */
export function elementTypeToRole(value: number): Role | undefined {
  const entry = (Object.entries(XCUIElementType) as Array<[XCUIElementTypeName, XCUIElementTypeValue]>)
    .find(([, v]) => v === value)
  if (!entry) return undefined
  const name = entry[0]
  const candidate = name as Role
  return KNOWN_ROLES.has(candidate) ? candidate : undefined
}

/**
 * String variant of elementTypeToRole — accepts the raw element-type name
 * (e.g. "button", "staticText") emitted on the wire by SimxRunner's GET /tree
 * route (v0.3 C1, TreeRoute.elementTypeName).
 *
 * Returns the same Role | undefined contract as elementTypeToRole(value:number):
 *   - rawType ∈ XCUIElementType ∧ ∈ KNOWN_ROLES → Role
 *   - rawType ∈ XCUIElementType but ∉ KNOWN_ROLES (e.g. 'other', 'group') → undefined
 *   - rawType ∉ XCUIElementType (typo or new iOS type) → undefined
 *
 * String variant exists because the wire emits string (TreeRoute.swift:160);
 * keeping a string accessor avoids name→number→name round-trip and avoids
 * double-maintaining one table on the TS side.
 */
export function elementTypeNameToRole(rawType: string): Role | undefined {
  if (!Object.prototype.hasOwnProperty.call(XCUIElementType, rawType)) return undefined
  const candidate = rawType as Role
  return KNOWN_ROLES.has(candidate) ? candidate : undefined
}

const KNOWN_ROLES: ReadonlySet<Role> = new Set<Role>([
  'button', 'link', 'textField', 'secureTextField', 'searchField',
  'switch', 'toggle', 'checkBox', 'image', 'staticText', 'tab',
  'tabBar', 'navigationBar', 'cell', 'alert', 'dialog', 'slider',
  'progressBar', 'picker', 'menu', 'menuItem', 'scrollView',
  'segmentedControl', 'table', 'collectionView', 'webView', 'keyboard',
])
