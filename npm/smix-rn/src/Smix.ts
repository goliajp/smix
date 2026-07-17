// The entry point. Resolution happens in the runner over HTTP, so callers
// pass the SelectorResolver in: HttpSelectorResolver against a live
// runner, MockSelectorResolver in tests.

import { App } from './App.js'
import { SmixNotImplementedError } from './Locator.js'
import type { LabelsResolver, SelectorResolver } from './SelectorResolver.js'
import type { SmixSimRuntime } from './SimRuntime.js'

export type AppTarget =
  | { kind: 'bundleId'; value: string }
  | { kind: 'appPath'; path: string }

export const bundleId = (value: string): AppTarget => ({ kind: 'bundleId', value })
export const appPath = (path: string): AppTarget => ({ kind: 'appPath', path })

/**
 * Top-level entry. Launches the target app via [runtime] and returns
 * an [App] handle wired with the given [resolver] for selector queries.
 */
export const Smix = {
  async launchApp(
    target: AppTarget,
    runtime: SmixSimRuntime,
    resolver: SelectorResolver,
    labelsResolver?: LabelsResolver,
  ): Promise<App> {
    switch (target.kind) {
      case 'bundleId': {
        await runtime.launch(target.value)
        return labelsResolver !== undefined
          ? new App(target.value, runtime, resolver, labelsResolver)
          : new App(target.value, runtime, resolver)
      }
      case 'appPath': {
        await runtime.launchFromPath(target.path)
        return labelsResolver !== undefined
          ? new App(target.path, runtime, resolver, labelsResolver)
          : new App(target.path, runtime, resolver)
      }
    }
  },
} as const

export { SmixNotImplementedError }
