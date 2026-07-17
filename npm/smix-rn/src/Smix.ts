// The entry point. Launching an app is a live driving action; with the
// fictional HTTP wire deleted and no FFI path in the TS SDK, it is pending
// the napi axis. launchApp throws SmixNotImplementedError rather than
// pretending to launch. The resolver seam it would wire into the returned
// App still works against the runner's served /select/resolve* routes.

import type { App } from './App.js'
import { SmixNotImplementedError } from './Locator.js'
import type { LabelsResolver, SelectorResolver } from './SelectorResolver.js'

export type AppTarget =
  | { kind: 'bundleId'; value: string }
  | { kind: 'appPath'; path: string }

export const bundleId = (value: string): AppTarget => ({ kind: 'bundleId', value })
export const appPath = (path: string): AppTarget => ({ kind: 'appPath', path })

/**
 * Top-level entry. Launches the target app and returns an [App] handle
 * wired with the given [resolver]. Pending the napi axis — throws
 * SmixNotImplementedError.
 */
export const Smix = {
  async launchApp(
    _target: AppTarget,
    _resolver: SelectorResolver,
    _labelsResolver?: LabelsResolver,
  ): Promise<App> {
    throw new SmixNotImplementedError('napi', 'Smix.launchApp')
  },
} as const

export { SmixNotImplementedError }
