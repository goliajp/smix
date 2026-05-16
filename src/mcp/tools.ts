import { z } from 'zod'
import { spawn as nodeSpawn } from 'node:child_process'
import { writeFile as fsWriteFile, unlink as fsUnlink } from 'node:fs/promises'
import { tmpdir as osTmpdir } from 'node:os'
import { join as pathJoin } from 'node:path'
import { randomBytes as cryptoRandomBytes } from 'node:crypto'
import type { SimctlClient } from '../sim/simctl.js'
import type { Driver, SwipeDirection, KeyName, Permission } from '../driver/types.js'
import type { Selector } from '../core/index.js'
import { describeQuery, resolveDevice } from './device-resolver.js'

export type ToolContentText = { type: 'text'; text: string }
export type ToolCallResult = {
  content: ToolContentText[]
  isError?: boolean
}

export type AcquireDriverFn = (udid: string) => Promise<Driver>

export type ToolContext = {
  client: SimctlClient
  acquireDriver: AcquireDriverFn
}

export type ToolJsonSchema = {
  type: 'object'
  properties: Record<string, unknown>
  required?: readonly string[]
  additionalProperties: boolean
}

export type ToolDef = {
  readonly name: string
  readonly description: string
  readonly inputSchema: z.ZodTypeAny
  readonly inputJsonSchema: ToolJsonSchema
  readonly outputJsonSchema?: ToolJsonSchema
  readonly handler: (args: unknown, ctx: ToolContext) => Promise<ToolCallResult>
}

function textError(msg: string): ToolCallResult {
  return { content: [{ type: 'text', text: msg }], isError: true }
}

function textOk(payload: unknown): ToolCallResult {
  return { content: [{ type: 'text', text: JSON.stringify(payload) }] }
}

function errMsg(err: unknown): string {
  return err instanceof Error ? err.message : String(err)
}

// ---------- ping (C1) ----------

export const pingTool: ToolDef = {
  name: 'ping',
  description:
    'Health check tool. Returns "pong". Used to verify the MCP transport is alive.',
  inputSchema: z.object({}).strict(),
  inputJsonSchema: {
    type: 'object',
    properties: {},
    additionalProperties: false,
  },
  handler: async (_args, _ctx): Promise<ToolCallResult> => {
    return { content: [{ type: 'text', text: 'pong' }] }
  },
}

// ---------- simulator_list ----------

const simulatorListInput = z.object({}).strict()

export const simulatorListTool: ToolDef = {
  name: 'simulator_list',
  description:
    'List all iOS simulators known to simctl, including udid, name, state, isAvailable, runtimeIdentifier, deviceTypeIdentifier.',
  inputSchema: simulatorListInput,
  inputJsonSchema: {
    type: 'object',
    properties: {},
    additionalProperties: false,
  },
  handler: async (_args, ctx): Promise<ToolCallResult> => {
    try {
      const devices = await ctx.client.listDevices()
      return textOk({ devices })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- simulator_boot ----------

const simulatorBootInput = z
  .object({
    udid: z.string().optional(),
    name: z.string().optional(),
    timeoutMs: z.number().int().positive().optional(),
  })
  .strict()
  .refine((v) => v.udid !== undefined || v.name !== undefined, {
    message: 'either udid or name is required',
  })

export const simulatorBootTool: ToolDef = {
  name: 'simulator_boot',
  description:
    'Boot an iOS simulator by udid or name. Blocks until ready (simctl bootstatus -b). No-op if already booted.',
  inputSchema: simulatorBootInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      name: { type: 'string' },
      timeoutMs: { type: 'number' },
    },
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof simulatorBootInput>
    const query = parsed.udid ?? parsed.name!
    try {
      const all = await ctx.client.listDevices()
      const candidates = resolveDevice(all, query)
      if (candidates.length === 0) {
        return textError(`device not found: ${describeQuery(query)}`)
      }
      if (candidates.length > 1) {
        return textError(
          `multiple devices match ${describeQuery(query)}: ${candidates.length} candidates`,
        )
      }
      const dev = candidates[0]!
      const alreadyBooted = dev.state === 'Booted'
      const start = Date.now()
      if (!alreadyBooted) {
        await ctx.client.bootAndWait(dev.udid, parsed.timeoutMs ?? 120_000)
      }
      const durationMs = alreadyBooted ? 0 : Date.now() - start
      return textOk({
        udid: dev.udid,
        name: dev.name,
        state: alreadyBooted ? dev.state : 'Booted',
        alreadyBooted,
        durationMs,
      })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- simulator_shutdown ----------

const simulatorShutdownInput = z
  .object({
    udid: z.string(),
  })
  .strict()

export const simulatorShutdownTool: ToolDef = {
  name: 'simulator_shutdown',
  description: 'Shutdown an iOS simulator by udid.',
  inputSchema: simulatorShutdownInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
    },
    required: ['udid'],
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof simulatorShutdownInput>
    try {
      await ctx.client.shutdown(parsed.udid)
      return textOk({ udid: parsed.udid, ok: true })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- app_launch ----------

const appLaunchInput = z
  .object({
    udid: z.string(),
    bundleId: z.string(),
    launchArguments: z.array(z.string()).optional(),
  })
  .strict()

export const appLaunchTool: ToolDef = {
  name: 'app_launch',
  description:
    'Launch an app on the given simulator by bundle id. Returns the launched pid. Optional launchArguments are passed to simctl launch.',
  inputSchema: appLaunchInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      bundleId: { type: 'string' },
      launchArguments: { type: 'array', items: { type: 'string' } },
    },
    required: ['udid', 'bundleId'],
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof appLaunchInput>
    try {
      const { pid } = await ctx.client.launch(
        parsed.udid,
        parsed.bundleId,
        parsed.launchArguments ?? [],
        {},
      )
      return textOk({ pid })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- app_terminate ----------

const appTerminateInput = z
  .object({
    udid: z.string(),
    bundleId: z.string(),
  })
  .strict()

export const appTerminateTool: ToolDef = {
  name: 'app_terminate',
  description: 'Terminate a running app on the given simulator by bundle id.',
  inputSchema: appTerminateInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      bundleId: { type: 'string' },
    },
    required: ['udid', 'bundleId'],
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof appTerminateInput>
    try {
      await ctx.client.terminate(parsed.udid, parsed.bundleId)
      return textOk({ udid: parsed.udid, bundleId: parsed.bundleId, ok: true })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- app_install ----------

const appInstallInput = z
  .object({
    udid: z.string(),
    appPath: z.string(),
  })
  .strict()

export const appInstallTool: ToolDef = {
  name: 'app_install',
  description: 'Install an .app bundle path onto the given simulator.',
  inputSchema: appInstallInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      appPath: { type: 'string' },
    },
    required: ['udid', 'appPath'],
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof appInstallInput>
    try {
      await ctx.client.install(parsed.udid, parsed.appPath)
      return textOk({ udid: parsed.udid, appPath: parsed.appPath, ok: true })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- app_uninstall ----------

const appUninstallInput = z
  .object({
    udid: z.string(),
    bundleId: z.string(),
  })
  .strict()

export const appUninstallTool: ToolDef = {
  name: 'app_uninstall',
  description: 'Uninstall an app from the given simulator by bundle id.',
  inputSchema: appUninstallInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      bundleId: { type: 'string' },
    },
    required: ['udid', 'bundleId'],
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof appUninstallInput>
    try {
      await ctx.client.uninstall(parsed.udid, parsed.bundleId)
      return textOk({ udid: parsed.udid, bundleId: parsed.bundleId, ok: true })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- selector zod (4 base + 8 modifier, string-only; RegExp deferred to v0.7+) ----------

const baseSelectorSchema: z.ZodType<unknown> = z.lazy(() =>
  z.union([
    z.object({ text: z.string() }).passthrough(),
    z.object({ id: z.string() }).passthrough(),
    z.object({ label: z.string() }).passthrough(),
    z
      .object({ role: z.string(), name: z.string().optional() })
      .passthrough(),
  ]),
)

const selectorSchema: z.ZodType<unknown> = z.lazy(() =>
  z.intersection(
    baseSelectorSchema,
    z.object({
      near: z.lazy(() => selectorSchema).optional(),
      below: z.lazy(() => selectorSchema).optional(),
      above: z.lazy(() => selectorSchema).optional(),
      leftOf: z.lazy(() => selectorSchema).optional(),
      rightOf: z.lazy(() => selectorSchema).optional(),
      inside: z.lazy(() => selectorSchema).optional(),
      nth: z.number().int().optional(),
      first: z.boolean().optional(),
      last: z.boolean().optional(),
    }),
  ),
)

// JSON Schema literal exposes only top-level object shape; MCP clients can pass
// nested modifier objects, which zod validates at runtime.
const SELECTOR_JSON_SCHEMA: Record<string, unknown> = {
  type: 'object',
  description:
    'Selector: one base form (text|id|label|role+name; strings only) plus optional modifiers (near|below|above|leftOf|rightOf|inside|nth|first|last). Modifiers nest the same shape recursively.',
}

// ScreenDescription wire shape (mirrors src/core/schemas.ts screenDescriptionSchema).
// Hand-mirrored JSON Schema rather than derived via zod-to-json-schema to keep
// 0 new npm dep (decision 8.B). schemas.test.ts round-trip catches drift.
const SCREEN_DESCRIPTION_OUTPUT_SCHEMA: ToolJsonSchema = {
  type: 'object',
  properties: {
    screenshot: { type: 'string', description: 'base64-encoded PNG' },
    elements: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          role: { type: 'string' },
          name: { type: 'string' },
          id: { type: 'string' },
          text: { type: 'string' },
          bounds: {
            type: 'object',
            properties: {
              x: { type: 'number' },
              y: { type: 'number' },
              w: { type: 'number' },
              h: { type: 'number' },
            },
            required: ['x', 'y', 'w', 'h'],
            additionalProperties: false,
          },
          enabled: { type: 'boolean' },
        },
        required: ['role', 'bounds', 'enabled'],
        additionalProperties: false,
      },
    },
    frontApp: { type: 'string' },
    summary: { type: 'string' },
    capturedAt: { type: 'number' },
  },
  required: ['screenshot', 'elements', 'frontApp', 'summary', 'capturedAt'],
  additionalProperties: false,
}

// withScreenAfter wire shape: {ok:true, screen:SD|null, screenError?:string}.
const SCREEN_WRAPPED_OUTPUT_SCHEMA: ToolJsonSchema = {
  type: 'object',
  properties: {
    ok: { type: 'boolean' },
    screen: { oneOf: [SCREEN_DESCRIPTION_OUTPUT_SCHEMA, { type: 'null' }] },
    screenError: { type: 'string' },
  },
  required: ['ok'],
  additionalProperties: false,
}

// ---------- screen_describe (C3) ----------

const screenDescribeInput = z
  .object({
    udid: z.string(),
    limit: z.number().int().positive().optional(),
  })
  .strict()

export const screenDescribeTool: ToolDef = {
  name: 'screen_describe',
  description:
    'Describe the current screen: returns ScreenDescription { screenshot (base64 PNG), elements (top N visible+enabled summaries in DFS pre-order), frontApp, summary, capturedAt }. Optional `limit` truncates elements client-side (driver returns up to 50).',
  inputSchema: screenDescribeInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      limit: { type: 'number' },
    },
    required: ['udid'],
    additionalProperties: false,
  },
  outputJsonSchema: SCREEN_DESCRIPTION_OUTPUT_SCHEMA,
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof screenDescribeInput>
    try {
      const driver = await ctx.acquireDriver(parsed.udid)
      const desc = await driver.describe()
      const elements =
        parsed.limit !== undefined
          ? desc.elements.slice(0, parsed.limit)
          : desc.elements
      return textOk({ ...desc, elements })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- screen_screenshot (C3) ----------

const screenScreenshotInput = z.object({ udid: z.string() }).strict()

export const screenScreenshotTool: ToolDef = {
  name: 'screen_screenshot',
  description:
    'Capture a PNG screenshot of the simulator screen. Returns { base64, byteLength }. base64 may be ~470KB for iPhone-class 1290×2796 frames.',
  inputSchema: screenScreenshotInput,
  inputJsonSchema: {
    type: 'object',
    properties: { udid: { type: 'string' } },
    required: ['udid'],
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof screenScreenshotInput>
    try {
      const driver = await ctx.acquireDriver(parsed.udid)
      const png = await driver.screenshot()
      return textOk({
        base64: png.toString('base64'),
        byteLength: png.byteLength,
      })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- screen_hierarchy (C3) ----------

const screenHierarchyInput = z.object({ udid: z.string() }).strict()

export const screenHierarchyTool: ToolDef = {
  name: 'screen_hierarchy',
  description:
    'Return the full accessibility tree (A11yNode { rawType, role?, identifier?, label?, value?, text?, bounds, enabled, selected, hasFocus, visible, children: A11yNode[] }) of the frontmost app. Requires a running SimxRunner XCUITest server bound to the runner port.',
  inputSchema: screenHierarchyInput,
  inputJsonSchema: {
    type: 'object',
    properties: { udid: { type: 'string' } },
    required: ['udid'],
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof screenHierarchyInput>
    try {
      const driver = await ctx.acquireDriver(parsed.udid)
      const tree = await driver.tree()
      return textOk({ tree })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- element_inspect (C3) ----------

const elementInspectInput = z
  .object({
    udid: z.string(),
    selector: selectorSchema,
  })
  .strict()

export const elementInspectTool: ToolDef = {
  name: 'element_inspect',
  description:
    'Resolve a single element by Selector and return its A11yNode (or { node: null } when no match). Selector supports 4 base forms (text|id|label|role+name; strings only — RegExp deferred to v0.7+) and 8 modifiers (near|below|above|leftOf|rightOf|inside|nth|first|last; modifiers nest the same shape recursively).',
  inputSchema: elementInspectInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      selector: SELECTOR_JSON_SCHEMA,
    },
    required: ['udid', 'selector'],
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof elementInspectInput>
    try {
      const driver = await ctx.acquireDriver(parsed.udid)
      const node = await driver.findOne(parsed.selector as Selector)
      return textOk({ node })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- withScreenAfter helper (C4) ----------

// Run a driver mutation, then optionally re-describe to give AI the new screen
// in a single round-trip. If the mutation succeeds but describe throws, the
// tool still reports ok:true with screen:null + screenError — the user-visible
// action already happened; surfacing it as isError would hide that.
async function withScreenAfter(
  ctx: ToolContext,
  udid: string,
  returnSummary: boolean,
  mutate: (driver: Driver) => Promise<void>,
): Promise<ToolCallResult> {
  let driver: Driver
  try {
    driver = await ctx.acquireDriver(udid)
  } catch (err) {
    return textError(errMsg(err))
  }
  try {
    await mutate(driver)
  } catch (err) {
    return textError(errMsg(err))
  }
  if (!returnSummary) {
    return textOk({ ok: true, screen: null })
  }
  try {
    const screen = await driver.describe()
    return textOk({ ok: true, screen })
  } catch (err) {
    return textOk({ ok: true, screen: null, screenError: errMsg(err) })
  }
}

// ---------- tap (C4) ----------

const tapInput = z
  .object({
    udid: z.string(),
    selector: selectorSchema,
    returnSummary: z.boolean().optional(),
  })
  .strict()

export const tapTool: ToolDef = {
  name: 'tap',
  description:
    'Single-tap the first element matching the Selector. Returns {ok:true, screen:ScreenDescription} after a fresh describe by default. Set returnSummary:false to skip the post-action describe (useful for batched fill→fill→tap chains). The tap fails (isError) if the selector resolves to zero elements.',
  inputSchema: tapInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      selector: SELECTOR_JSON_SCHEMA,
      returnSummary: { type: 'boolean' },
    },
    required: ['udid', 'selector'],
    additionalProperties: false,
  },
  outputJsonSchema: SCREEN_WRAPPED_OUTPUT_SCHEMA,
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof tapInput>
    return withScreenAfter(ctx, parsed.udid, parsed.returnSummary !== false, (d) =>
      d.tap(parsed.selector as Selector),
    )
  },
}

// ---------- double_tap (C4) ----------

const doubleTapInput = z
  .object({
    udid: z.string(),
    selector: selectorSchema,
    returnSummary: z.boolean().optional(),
  })
  .strict()

export const doubleTapTool: ToolDef = {
  name: 'double_tap',
  description:
    'Double-tap the first element matching the Selector. Returns {ok:true, screen} by default.',
  inputSchema: doubleTapInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      selector: SELECTOR_JSON_SCHEMA,
      returnSummary: { type: 'boolean' },
    },
    required: ['udid', 'selector'],
    additionalProperties: false,
  },
  outputJsonSchema: SCREEN_WRAPPED_OUTPUT_SCHEMA,
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof doubleTapInput>
    return withScreenAfter(ctx, parsed.udid, parsed.returnSummary !== false, (d) =>
      d.doubleTap(parsed.selector as Selector),
    )
  },
}

// ---------- long_press (C4) ----------

const longPressInput = z
  .object({
    udid: z.string(),
    selector: selectorSchema,
    durationMs: z.number().int().positive().optional(),
    returnSummary: z.boolean().optional(),
  })
  .strict()

export const longPressTool: ToolDef = {
  name: 'long_press',
  description:
    'Long-press the first element matching the Selector. Optional durationMs controls hold time (driver default applies when omitted).',
  inputSchema: longPressInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      selector: SELECTOR_JSON_SCHEMA,
      durationMs: { type: 'number' },
      returnSummary: { type: 'boolean' },
    },
    required: ['udid', 'selector'],
    additionalProperties: false,
  },
  outputJsonSchema: SCREEN_WRAPPED_OUTPUT_SCHEMA,
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof longPressInput>
    return withScreenAfter(ctx, parsed.udid, parsed.returnSummary !== false, (d) =>
      d.longPress(parsed.selector as Selector, parsed.durationMs),
    )
  },
}

// ---------- fill (C4) ----------

const fillInput = z
  .object({
    udid: z.string(),
    selector: selectorSchema,
    value: z.string(),
    returnSummary: z.boolean().optional(),
  })
  .strict()

export const fillTool: ToolDef = {
  name: 'fill',
  description:
    'Type text into the first text field matching the Selector. The wire field `value` maps to driver.fill(selector, text); empty strings are allowed (passed through unchanged).',
  inputSchema: fillInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      selector: SELECTOR_JSON_SCHEMA,
      value: { type: 'string' },
      returnSummary: { type: 'boolean' },
    },
    required: ['udid', 'selector', 'value'],
    additionalProperties: false,
  },
  outputJsonSchema: SCREEN_WRAPPED_OUTPUT_SCHEMA,
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof fillInput>
    return withScreenAfter(ctx, parsed.udid, parsed.returnSummary !== false, (d) =>
      d.fill(parsed.selector as Selector, parsed.value),
    )
  },
}

// ---------- swipe (C4) ----------

const swipeDirectionSchema = z.enum(['up', 'down', 'left', 'right'])

const swipeInput = z
  .object({
    udid: z.string(),
    direction: swipeDirectionSchema,
    from: selectorSchema.optional(),
    returnSummary: z.boolean().optional(),
  })
  .strict()

export const swipeTool: ToolDef = {
  name: 'swipe',
  description:
    'Swipe in a cardinal direction. Optional `from` Selector picks the starting element (otherwise the driver picks a sensible centre). distance/duration are deferred to v0.7+ (driver interface limits us to direction + optional from selector).',
  inputSchema: swipeInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      direction: { type: 'string', enum: ['up', 'down', 'left', 'right'] },
      from: SELECTOR_JSON_SCHEMA,
      returnSummary: { type: 'boolean' },
    },
    required: ['udid', 'direction'],
    additionalProperties: false,
  },
  outputJsonSchema: SCREEN_WRAPPED_OUTPUT_SCHEMA,
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof swipeInput>
    return withScreenAfter(ctx, parsed.udid, parsed.returnSummary !== false, (d) =>
      d.swipe(
        parsed.direction as SwipeDirection,
        parsed.from as Selector | undefined,
      ),
    )
  },
}

// ---------- scroll_to (C4) ----------

const scrollToInput = z
  .object({
    udid: z.string(),
    selector: selectorSchema,
    returnSummary: z.boolean().optional(),
  })
  .strict()

export const scrollToTool: ToolDef = {
  name: 'scroll_to',
  description:
    'Scroll the nearest scroll container until the element matching the Selector becomes visible. The element must exist somewhere in the a11y tree.',
  inputSchema: scrollToInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      selector: SELECTOR_JSON_SCHEMA,
      returnSummary: { type: 'boolean' },
    },
    required: ['udid', 'selector'],
    additionalProperties: false,
  },
  outputJsonSchema: SCREEN_WRAPPED_OUTPUT_SCHEMA,
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof scrollToInput>
    return withScreenAfter(ctx, parsed.udid, parsed.returnSummary !== false, (d) =>
      d.scrollTo(parsed.selector as Selector),
    )
  },
}

// ---------- key_press (C4) ----------

const keyNameSchema = z.enum([
  'return',
  'delete',
  'tab',
  'space',
  'escape',
  'arrowUp',
  'arrowDown',
  'arrowLeft',
  'arrowRight',
])

const keyPressInput = z
  .object({
    udid: z.string(),
    key: keyNameSchema,
    returnSummary: z.boolean().optional(),
  })
  .strict()

export const keyPressTool: ToolDef = {
  name: 'key_press',
  description:
    'Press one of the 9 hardware/keyboard keys: return | delete | tab | space | escape | arrowUp | arrowDown | arrowLeft | arrowRight (string-only enum; "enter" is not a valid value — use "return"). Driver routes this to the appropriate XCUITest / HID call depending on focus.',
  inputSchema: keyPressInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      key: {
        type: 'string',
        enum: [
          'return',
          'delete',
          'tab',
          'space',
          'escape',
          'arrowUp',
          'arrowDown',
          'arrowLeft',
          'arrowRight',
        ],
      },
      returnSummary: { type: 'boolean' },
    },
    required: ['udid', 'key'],
    additionalProperties: false,
  },
  outputJsonSchema: SCREEN_WRAPPED_OUTPUT_SCHEMA,
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof keyPressInput>
    return withScreenAfter(ctx, parsed.udid, parsed.returnSummary !== false, (d) =>
      d.pressKey(parsed.key as KeyName),
    )
  },
}

// ---------- find_and_tap (C5) ----------

const findAndTapInput = z
  .object({
    udid: z.string(),
    selector: selectorSchema,
    timeoutMs: z.number().int().positive().optional(),
    returnSummary: z.boolean().optional(),
  })
  .strict()

export const findAndTapTool: ToolDef = {
  name: 'find_and_tap',
  description:
    'Wait for an element matching the Selector to exist (driver.waitFor with optional timeoutMs), then tap it, then return {ok:true, screen:ScreenDescription} (set returnSummary:false to skip the post-action describe). isError if waitFor times out before tap.',
  inputSchema: findAndTapInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      selector: SELECTOR_JSON_SCHEMA,
      timeoutMs: { type: 'number' },
      returnSummary: { type: 'boolean' },
    },
    required: ['udid', 'selector'],
    additionalProperties: false,
  },
  outputJsonSchema: SCREEN_WRAPPED_OUTPUT_SCHEMA,
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof findAndTapInput>
    return withScreenAfter(
      ctx,
      parsed.udid,
      parsed.returnSummary !== false,
      async (d) => {
        await d.waitFor(parsed.selector as Selector, parsed.timeoutMs)
        await d.tap(parsed.selector as Selector)
      },
    )
  },
}

// ---------- wait_for (C5) ----------

const waitForInput = z
  .object({
    udid: z.string(),
    selector: selectorSchema,
    timeoutMs: z.number().int().positive().optional(),
  })
  .strict()

export const waitForTool: ToolDef = {
  name: 'wait_for',
  description:
    'Wait for an element matching the Selector to exist. Returns {node: A11yNode}. isError if waitFor times out. Does not capture a screen (use find_and_tap if you need both).',
  inputSchema: waitForInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      selector: SELECTOR_JSON_SCHEMA,
      timeoutMs: { type: 'number' },
    },
    required: ['udid', 'selector'],
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof waitForInput>
    try {
      const driver = await ctx.acquireDriver(parsed.udid)
      const node = await driver.waitFor(parsed.selector as Selector, parsed.timeoutMs)
      return textOk({ node })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- flow_run (C5) ----------

const flowStepSchema = z
  .object({
    action: z.string(),
    args: z.record(z.unknown()).optional(),
  })
  .strict()

const flowRunInput = z
  .object({
    udid: z.string(),
    steps: z.array(flowStepSchema),
    bailOnError: z.boolean().optional(),
  })
  .strict()

// Dispatch a sub-tool by name from within flow_run.
// Returns the sub-tool ToolCallResult as-is; flow_run aggregates pass/fail counts.
// Excludes `flow_run` itself to prevent unbounded recursion (decision 4.A.7).
async function dispatchSubTool(
  action: string,
  args: unknown,
  ctx: ToolContext,
): Promise<ToolCallResult> {
  if (action === 'flow_run') {
    return textError('flow_run cannot dispatch flow_run (recursion forbidden)')
  }
  const tool = ALL_TOOLS.find((t) => t.name === action)
  if (tool === undefined) {
    return textError(`unknown tool: ${action}`)
  }
  const parsed = tool.inputSchema.safeParse(args)
  if (!parsed.success) {
    return textError(`invalid arguments for ${action}: ${parsed.error.message}`)
  }
  return tool.handler(parsed.data, ctx)
}

export const flowRunTool: ToolDef = {
  name: 'flow_run',
  description:
    'Run a serialized sequence of MCP tool calls. Each step is {action: <tool name>, args: <tool arguments>}. The top-level udid is injected into each step args if step.args.udid is missing. Default bailOnError=true stops at first failure. Returns {passed, failed, stepsExecuted, lastError?, lastResult?}. flow_run cannot dispatch flow_run.',
  inputSchema: flowRunInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      steps: {
        type: 'array',
        items: {
          type: 'object',
          properties: {
            action: { type: 'string' },
            args: { type: 'object' },
          },
          required: ['action'],
          additionalProperties: false,
        },
      },
      bailOnError: { type: 'boolean' },
    },
    required: ['udid', 'steps'],
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof flowRunInput>
    const bail = parsed.bailOnError !== false
    let passed = 0
    let failed = 0
    let stepsExecuted = 0
    let lastError: string | undefined
    let lastResult: ToolCallResult | undefined
    for (const step of parsed.steps) {
      const stepArgs = { udid: parsed.udid, ...(step.args ?? {}) }
      const result = await dispatchSubTool(step.action, stepArgs, ctx)
      stepsExecuted += 1
      lastResult = result
      if (result.isError === true) {
        failed += 1
        const first = result.content[0]
        const txt = first && first.type === 'text' ? first.text : 'unknown error'
        lastError = txt
        // Recursion attempt surfaces directly as isError at the flow_run level
        // (don't aggregate it as a normal sub-tool failure — it's a contract violation).
        if (step.action === 'flow_run') {
          return textError(txt)
        }
        if (bail) {
          break
        }
      } else {
        passed += 1
      }
    }
    const payload: Record<string, unknown> = { passed, failed, stepsExecuted }
    if (lastError !== undefined) payload.lastError = lastError
    if (lastResult !== undefined) payload.lastResult = lastResult
    return textOk(payload)
  },
}

// ---------- open_url (C5) ----------

const openUrlInput = z
  .object({
    udid: z.string(),
    url: z.string(),
  })
  .strict()

export const openUrlTool: ToolDef = {
  name: 'open_url',
  description:
    'Open a URL (http(s) or custom scheme) on the simulator. Wraps simctl openurl. Returns {ok:true}.',
  inputSchema: openUrlInput,
  inputJsonSchema: {
    type: 'object',
    properties: { udid: { type: 'string' }, url: { type: 'string' } },
    required: ['udid', 'url'],
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof openUrlInput>
    try {
      const driver = await ctx.acquireDriver(parsed.udid)
      await driver.openUrl(parsed.url)
      return textOk({ ok: true })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- pasteboard_set (C5) ----------

const pasteboardSetInput = z
  .object({
    udid: z.string(),
    value: z.string(),
  })
  .strict()

export const pasteboardSetTool: ToolDef = {
  name: 'pasteboard_set',
  description:
    'Set the simulator pasteboard text (wraps simctl pbcopy). Empty strings are allowed. Returns {ok:true}.',
  inputSchema: pasteboardSetInput,
  inputJsonSchema: {
    type: 'object',
    properties: { udid: { type: 'string' }, value: { type: 'string' } },
    required: ['udid', 'value'],
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof pasteboardSetInput>
    try {
      const driver = await ctx.acquireDriver(parsed.udid)
      await driver.pasteboardSet(parsed.value)
      return textOk({ ok: true })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- pasteboard_get (C5) ----------

const pasteboardGetInput = z
  .object({
    udid: z.string(),
  })
  .strict()

export const pasteboardGetTool: ToolDef = {
  name: 'pasteboard_get',
  description:
    'Read the simulator pasteboard text (wraps simctl pbpaste). Returns {value:<string>}.',
  inputSchema: pasteboardGetInput,
  inputJsonSchema: {
    type: 'object',
    properties: { udid: { type: 'string' } },
    required: ['udid'],
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof pasteboardGetInput>
    try {
      const driver = await ctx.acquireDriver(parsed.udid)
      const value = await driver.pasteboardGet()
      return textOk({ value })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- permissions_grant (C5) ----------

const permissionSchema = z.enum([
  'camera',
  'photos',
  'location',
  'locationAlways',
  'notifications',
  'microphone',
  'contacts',
  'calendar',
  'reminders',
  'bluetooth',
  'motion',
  'faceId',
])

const permissionsGrantInput = z
  .object({
    udid: z.string(),
    bundleId: z.string(),
    permission: permissionSchema,
  })
  .strict()

export const permissionsGrantTool: ToolDef = {
  name: 'permissions_grant',
  description:
    'Grant a single iOS permission to an app bundle: one of camera | photos | location | locationAlways | notifications | microphone | contacts | calendar | reminders | bluetooth | motion | faceId (12 literal enum; mirrors Driver Permission type). Grant multiple permissions by calling repeatedly or via flow_run. bundleId is required (MCP surface is stateless and does not honour driver-internal lastLaunchedBundleId fallback). Returns {ok:true}.',
  inputSchema: permissionsGrantInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      bundleId: { type: 'string' },
      permission: {
        type: 'string',
        enum: [
          'camera',
          'photos',
          'location',
          'locationAlways',
          'notifications',
          'microphone',
          'contacts',
          'calendar',
          'reminders',
          'bluetooth',
          'motion',
          'faceId',
        ],
      },
    },
    required: ['udid', 'bundleId', 'permission'],
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof permissionsGrantInput>
    try {
      const driver = await ctx.acquireDriver(parsed.udid)
      await driver.grantPermission(parsed.permission as Permission, parsed.bundleId)
      return textOk({ ok: true })
    } catch (err) {
      return textError(errMsg(err))
    }
  },
}

// ---------- explain_screen (C6) ----------

// Module-private injectables. Prod path uses stdlib; tests swap them via
// __set*Impl exports below. Decision 14.C-D: module-private + __ prefix on
// test-only setters keeps the public surface clean while allowing single-file
// unit-test isolation (no monkey-patch of node:child_process globals).
let spawnImpl: typeof nodeSpawn = nodeSpawn
let writeFileImpl: typeof fsWriteFile = fsWriteFile
let unlinkImpl: typeof fsUnlink = fsUnlink
let tmpdirImpl: typeof osTmpdir = osTmpdir
let randomBytesImpl: typeof cryptoRandomBytes = cryptoRandomBytes

/** @internal test-only */
export function __setSpawnImpl(fn: typeof nodeSpawn): void {
  spawnImpl = fn
}
/** @internal test-only */
export function __resetSpawnImpl(): void {
  spawnImpl = nodeSpawn
}
/** @internal test-only */
export function __setWriteFileImpl(fn: typeof fsWriteFile): void {
  writeFileImpl = fn
}
/** @internal test-only */
export function __resetWriteFileImpl(): void {
  writeFileImpl = fsWriteFile
}
/** @internal test-only */
export function __setUnlinkImpl(fn: typeof fsUnlink): void {
  unlinkImpl = fn
}
/** @internal test-only */
export function __resetUnlinkImpl(): void {
  unlinkImpl = fsUnlink
}
/** @internal test-only */
export function __setTmpdirImpl(fn: typeof osTmpdir): void {
  tmpdirImpl = fn
}
/** @internal test-only */
export function __resetTmpdirImpl(): void {
  tmpdirImpl = osTmpdir
}
/** @internal test-only */
export function __setRandomBytesImpl(fn: typeof cryptoRandomBytes): void {
  randomBytesImpl = fn
}
/** @internal test-only */
export function __resetRandomBytesImpl(): void {
  randomBytesImpl = cryptoRandomBytes
}

type ClaudeRunResult =
  | { kind: 'ok'; stdout: string }
  | { kind: 'not-installed' }
  | { kind: 'exit-nonzero'; code: number; stderr: string }
  | { kind: 'timed-out'; timeoutMs: number }

async function spawnClaudeExplain(
  prompt: string,
  timeoutMs: number,
): Promise<ClaudeRunResult> {
  return new Promise((resolve) => {
    const ac = new AbortController()
    let timedOut = false
    const t = setTimeout(() => {
      timedOut = true
      ac.abort()
    }, timeoutMs)
    let child: ReturnType<typeof spawnImpl>
    try {
      child = spawnImpl(
        'claude',
        ['-p', prompt, '--tools', 'Read', '--output-format', 'text'],
        { stdio: ['ignore', 'pipe', 'pipe'], signal: ac.signal },
      )
    } catch (err) {
      clearTimeout(t)
      const e = err as NodeJS.ErrnoException
      if (e.code === 'ENOENT') return resolve({ kind: 'not-installed' })
      return resolve({ kind: 'exit-nonzero', code: -1, stderr: e.message })
    }
    let stdout = ''
    let stderr = ''
    child.stdout?.on('data', (b: Buffer) => {
      stdout += b.toString('utf8')
    })
    child.stderr?.on('data', (b: Buffer) => {
      stderr += b.toString('utf8')
    })
    child.on('error', (err) => {
      clearTimeout(t)
      const e = err as NodeJS.ErrnoException
      if (e.code === 'ENOENT') return resolve({ kind: 'not-installed' })
      if (timedOut) return resolve({ kind: 'timed-out', timeoutMs })
      resolve({ kind: 'exit-nonzero', code: -1, stderr: e.message })
    })
    child.on('close', (code) => {
      clearTimeout(t)
      if (timedOut) return resolve({ kind: 'timed-out', timeoutMs })
      if (code === 0) return resolve({ kind: 'ok', stdout })
      const tail = stderr.length > 256 ? stderr.slice(-256) : stderr
      resolve({ kind: 'exit-nonzero', code: code ?? -1, stderr: tail.trim() })
    })
  })
}

const DEFAULT_EXPLAIN_QUESTION =
  "Describe what's on this screen in detail. List visible UI elements, their text, and likely interaction targets."
const CLAUDE_INSTALL_URL =
  'https://docs.claude.com/en/docs/claude-code/setup'

const explainScreenInput = z
  .object({
    udid: z.string(),
    question: z.string().optional(),
    timeoutMs: z.number().int().positive().optional(),
  })
  .strict()

export const explainScreenTool: ToolDef = {
  name: 'explain_screen',
  description:
    'Use a vision-capable LLM (local `claude` CLI subprocess) to describe what is on the simulator screen. Captures a fresh screenshot, writes it to a temp PNG, then spawns `claude -p <prompt-with-path> --tools Read --output-format text` so claude can read the image. Returns {description, raw_output}. Default question asks for visible UI elements; pass `question` to ask something specific. Default timeoutMs=30000. isError when claude is not installed, exits non-zero, or times out.',
  inputSchema: explainScreenInput,
  inputJsonSchema: {
    type: 'object',
    properties: {
      udid: { type: 'string' },
      question: { type: 'string' },
      timeoutMs: { type: 'number' },
    },
    required: ['udid'],
    additionalProperties: false,
  },
  handler: async (args, ctx): Promise<ToolCallResult> => {
    const parsed = args as z.infer<typeof explainScreenInput>
    const question = parsed.question ?? DEFAULT_EXPLAIN_QUESTION
    const timeoutMs = parsed.timeoutMs ?? 30_000

    let driver: Driver
    try {
      driver = await ctx.acquireDriver(parsed.udid)
    } catch (err) {
      return textError(errMsg(err))
    }

    let png: Buffer
    try {
      png = await driver.screenshot()
    } catch (err) {
      return textError(`screenshot failed: ${errMsg(err)}`)
    }

    const suffix = randomBytesImpl(4).toString('hex')
    const udidPrefix = parsed.udid.replace(/[^a-zA-Z0-9]/g, '').slice(0, 8)
    const tmpPath = pathJoin(
      tmpdirImpl(),
      `simx-explain-${udidPrefix}-${suffix}.png`,
    )

    let wrote = false
    try {
      try {
        await writeFileImpl(tmpPath, png)
        wrote = true
      } catch (err) {
        return textError(`tmp write failed: ${errMsg(err)}`)
      }
      const prompt = `Look at the screenshot at ${tmpPath}. ${question}`
      const result = await spawnClaudeExplain(prompt, timeoutMs)
      if (result.kind === 'not-installed') {
        return textError(
          `claude CLI not found in PATH. Install Claude Code: ${CLAUDE_INSTALL_URL}`,
        )
      }
      if (result.kind === 'timed-out') {
        return textError(
          `explain_screen timed out after ${result.timeoutMs}ms`,
        )
      }
      if (result.kind === 'exit-nonzero') {
        return textError(
          `claude CLI exited with code ${result.code}: ${result.stderr}`,
        )
      }
      const trimmed = result.stdout.trim()
      return textOk({ description: trimmed, raw_output: trimmed })
    } finally {
      if (wrote) {
        try {
          await unlinkImpl(tmpPath)
        } catch {
          // swallow ENOENT / other cleanup failures — tmp may have been
          // removed by an external janitor; we don't want cleanup noise to
          // mask the real handler result.
        }
      }
    }
  },
}

export const ALL_TOOLS: readonly ToolDef[] = [
  pingTool,
  simulatorListTool,
  simulatorBootTool,
  simulatorShutdownTool,
  appLaunchTool,
  appTerminateTool,
  appInstallTool,
  appUninstallTool,
  screenDescribeTool,
  screenScreenshotTool,
  screenHierarchyTool,
  elementInspectTool,
  tapTool,
  doubleTapTool,
  longPressTool,
  fillTool,
  swipeTool,
  scrollToTool,
  keyPressTool,
  findAndTapTool,
  waitForTool,
  flowRunTool,
  openUrlTool,
  pasteboardSetTool,
  pasteboardGetTool,
  permissionsGrantTool,
  explainScreenTool,
] as const
