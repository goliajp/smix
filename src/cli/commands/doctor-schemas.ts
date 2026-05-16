/// v0.7 C4 — single-source-of-truth zod schemas for `simx doctor --json`
/// shape.  Independent CLI-local SoT (decision 3.A) — kept outside
/// `src/core/schemas.ts` (which is reserved for MCP wire / SDK shared shapes).
///
/// Exports 7 schemas + 1 internal name enum + 8 z.infer type re-exports.
/// `src/cli/commands/doctor.ts` re-exports these types so downstream imports
/// (`src/cli/index.ts` + `src/cli/__tests__/doctor.test.ts`) stay unchanged.
import { z } from 'zod'

export const compatibilitySchema = z
  .object({
    status: z.enum(['supported', 'unsupported', 'partial', 'unknown']),
    required: z.string().optional(),
    actual: z.string().optional(),
    message: z.string().optional(),
  })
  .strict()

export const runtimesValueSchema = z
  .object({
    count: z.number(),
    items: z.array(
      z
        .object({
          identifier: z.string(),
          name: z.string(),
          version: z.string(),
          isAvailable: z.boolean(),
        })
        .strict(),
    ),
  })
  .strict()

export const claudeValueSchema = z
  .object({
    path: z.string().optional(),
    loggedIn: z.boolean(),
    failureDetail: z.string().optional(),
  })
  .strict()

export const hidChannelAvailabilitySchema = z
  .object({
    digitizer: z.boolean(),
    indigo9: z.boolean(),
  })
  .strict()

export const axpCapabilitySchema = z
  .object({
    framework: z.object({ path: z.string(), loaded: z.boolean() }).strict(),
    classes: z
      .object({ resolved: z.array(z.string()), missing: z.array(z.string()) })
      .strict(),
    selectors: z
      .object({ resolved: z.array(z.string()), missing: z.array(z.string()) })
      .strict(),
    sharedInstance: z.object({ available: z.boolean() }).strict(),
  })
  .strict()

export const doctorCheckNameSchema = z.enum([
  'xcode',
  'runtimes',
  'claude',
  'bun',
  'hid',
  'axp',
])

export const doctorCheckSchema = z
  .object({
    name: doctorCheckNameSchema,
    ok: z.boolean(),
    value: z
      .union([
        z.string(),
        runtimesValueSchema,
        claudeValueSchema,
        hidChannelAvailabilitySchema,
        axpCapabilitySchema,
      ])
      .optional(),
    message: z.string().optional(),
    compatibility: compatibilitySchema.optional(),
  })
  .strict()

export const doctorReportSchema = z
  .object({
    exitCode: z.union([z.literal(0), z.literal(1)]),
    checks: z.array(doctorCheckSchema),
  })
  .strict()

export type Compatibility = z.infer<typeof compatibilitySchema>
export type RuntimesValue = z.infer<typeof runtimesValueSchema>
export type ClaudeValue = z.infer<typeof claudeValueSchema>
export type HidChannelAvailability = z.infer<typeof hidChannelAvailabilitySchema>
export type AxpCapability = z.infer<typeof axpCapabilitySchema>
export type DoctorCheckName = z.infer<typeof doctorCheckNameSchema>
export type DoctorCheck = z.infer<typeof doctorCheckSchema>
export type DoctorReport = z.infer<typeof doctorReportSchema>
