// The entry point. Launching an app opens a napi session on the driver,
// launches the app in it, and hands back a wired App. `appPath` targets need
// a host-side install (no runner wire), so they throw a clear host-side error
// rather than pretending; the bundleId path is the golden path.

import { App } from './App.js'
import { SmixNotImplementedError } from './Locator.js'
import type { NodeDriver } from './NodeDriver.js'
import type { LabelsResolver, SelectorResolver } from './SelectorResolver.js'

export type AppTarget =
  | { kind: 'bundleId'; value: string }
  | { kind: 'appPath'; path: string }

export const bundleId = (value: string): AppTarget => ({ kind: 'bundleId', value })
export const appPath = (path: string): AppTarget => ({ kind: 'appPath', path })

/**
 * Top-level entry. Opens a session on `driver` bound to the target's bundle
 * id, launches its app, and returns an [App] handle wired with the driver,
 * session, and the given [resolver].
 */
export const Smix = {
  async launchApp(
    target: AppTarget,
    driver: NodeDriver,
    resolver: SelectorResolver,
    labelsResolver?: LabelsResolver,
  ): Promise<App> {
    if (target.kind !== 'bundleId') {
      throw new SmixNotImplementedError('host', 'Smix.launchApp(appPath)')
    }
    const session = await driver.openSession(target.value)
    await session.launchApp()
    return labelsResolver === undefined
      ? new App(target.value, driver, session, resolver)
      : new App(target.value, driver, session, resolver, labelsResolver)
  },
} as const

export { SmixNotImplementedError }
