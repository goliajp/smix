import { z } from 'zod'

export const rectSchema = z
  .object({
    x: z.number(),
    y: z.number(),
    w: z.number(),
    h: z.number(),
  })
  .strict()

// Role enum mirrors src/core/role.ts at write-time. schemas.ts is the
// single SoT; role.ts becomes a thin type re-export.
export const roleSchema = z.enum([
  'button',
  'link',
  'textField',
  'secureTextField',
  'searchField',
  'switch',
  'toggle',
  'checkBox',
  'radio',
  'image',
  'staticText',
  'tab',
  'tabBar',
  'navigationBar',
  'cell',
  'alert',
  'dialog',
  'slider',
  'progressBar',
  'picker',
  'menu',
  'menuItem',
  'scrollView',
  'segmentedControl',
  'table',
  'collectionView',
  'webView',
  'keyboard',
])

export const elementSummarySchema = z
  .object({
    role: z.union([roleSchema, z.literal('unknown')]),
    name: z.string().optional(),
    id: z.string().optional(),
    text: z.string().optional(),
    bounds: rectSchema,
    enabled: z.boolean(),
  })
  .strict()

// z.lazy infers `unknown`; we hand-mirror the recursive type below.
// schemas.test.ts case 5 round-trip catches drift between schema and type.
export const a11yNodeSchema: z.ZodType<unknown> = z.lazy(() =>
  z
    .object({
      rawType: z.string(),
      role: roleSchema.optional(),
      identifier: z.string().optional(),
      label: z.string().optional(),
      value: z.string().optional(),
      text: z.string().optional(),
      bounds: rectSchema,
      enabled: z.boolean(),
      selected: z.boolean(),
      hasFocus: z.boolean(),
      visible: z.boolean(),
      children: z.array(a11yNodeSchema),
    })
    .strict(),
)

export const screenDescriptionSchema = z
  .object({
    screenshot: z.string(),
    elements: z.array(elementSummarySchema),
    frontApp: z.string(),
    summary: z.string(),
    capturedAt: z.number(),
  })
  .strict()

export const failureCodeSchema = z.enum([
  'ELEMENT_NOT_FOUND',
  'NOT_VISIBLE',
  'NOT_ENABLED',
  'AMBIGUOUS',
  'TIMEOUT',
  'ASSERTION_FAILED',
  'APP_NOT_RUNNING',
  'SIMULATOR_NOT_BOOTED',
  'DRIVER_ERROR',
])

// Selector is opaque at this layer — deep zod lives in src/mcp/tools.ts
// selectorSchema. Use z.unknown() so consumers typecheck against the
// canonical Selector type from src/core/selector.ts; runtime validation of
// selector internals stays at the MCP boundary.
export const serializedFailureSchema = z
  .object({
    ok: z.literal(false),
    code: failureCodeSchema,
    message: z.string(),
    selector: z.unknown().optional(),
    suggestions: z.array(z.string()),
    visibleElements: z.array(elementSummarySchema),
    hint: z.string().optional(),
    screenshot: z.string().optional(),
  })
  .strict()

// ----- Inferred TypeScript types (single source) -----
export type Rect = z.infer<typeof rectSchema>
export type Role = z.infer<typeof roleSchema>
export type ElementSummary = z.infer<typeof elementSummarySchema>

// A11yNode: z.lazy infers `unknown`, so we hand-mirror the recursive shape.
export type A11yNode = {
  rawType: string
  role?: Role
  identifier?: string
  label?: string
  value?: string
  text?: string
  bounds: Rect
  enabled: boolean
  selected: boolean
  hasFocus: boolean
  visible: boolean
  children: A11yNode[]
}

export type ScreenDescription = z.infer<typeof screenDescriptionSchema>
export type FailureCode = z.infer<typeof failureCodeSchema>

// Selector wire is opaque (unknown) but consumers type-check against
// Selector from src/core/selector.ts. Intersect to bridge.
export type SerializedFailure = Omit<
  z.infer<typeof serializedFailureSchema>,
  'selector'
> & {
  selector?: import('./selector.js').Selector
}
