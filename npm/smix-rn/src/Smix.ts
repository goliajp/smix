// The entry point. Launching an app opens a napi session on the driver,
// launches the app in it, and hands back a wired App. `appPath` targets need
// a host-side install (no runner wire), so they throw a clear host-side error
// rather than pretending; the bundleId path is the golden path.

import { App } from './App.js'
import { loadNodeDriver } from './loadNodeDriver.js'
import { SmixNotImplementedError } from './Locator.js'
import type { NodeDriver } from './NodeDriver.js'
import type { LabelsResolver, SelectorResolver } from './SelectorResolver.js'

export type AppTarget =
  | { kind: 'bundleId'; value: string }
  | { kind: 'appPath'; path: string }

export const bundleId = (value: string): AppTarget => ({ kind: 'bundleId', value })
export const appPath = (path: string): AppTarget => ({ kind: 'appPath', path })

/**
 * Top-level entry. Opens a session bound to the target's bundle id, launches
 * its app, and returns an [App] handle wired with the resolver. The driver
 * defaults to the real napi addon (`loadNodeDriver()`); pass `options.driver`
 * to inject one (tests, or a custom transport).
 */
export const Smix = {
  async launchApp(
    target: AppTarget,
    resolver: SelectorResolver,
    options?: { driver?: NodeDriver; labelsResolver?: LabelsResolver },
  ): Promise<App> {
    if (target.kind !== 'bundleId') {
      throw new SmixNotImplementedError('host', 'Smix.launchApp(appPath)')
    }
    const driver = options?.driver ?? (await loadNodeDriver())
    const session = await driver.openSession(target.value)
    await session.launchApp()
    return options?.labelsResolver === undefined
      ? new App(target.value, driver, session, resolver)
      : new App(target.value, driver, session, resolver, options.labelsResolver)
  },
} as const

export { SmixNotImplementedError }
