import { describe, expect, it } from 'vitest'
import { App } from '../App.js'
import { MockNodeDriver, MockNodeSession } from '../NodeDriver.js'
import { MockSelectorResolver } from '../SelectorResolver.js'
import { Selector, encodeSelectorJson } from '../Selector.js'
import { Smix, bundleId } from '../Smix.js'
import { ExpectationFailure } from '../ExpectationFailure.js'

function makeApp(driver = new MockNodeDriver(), resolver = new MockSelectorResolver()) {
  return {
    driver,
    resolver,
    app: new App('bid', driver, driver.session, resolver.resolve),
  }
}

describe('App live sense (C3)', () => {
  it('snapshotTree returns the parsed tree from the driver', async () => {
    const driver = new MockNodeDriver()
    driver.treeJson = '{"rawType":"application","identifier":"root","enabled":true,"selected":false,"hasFocus":false,"visible":true,"bounds":{"x":0,"y":0,"w":0,"h":0}}'
    const { app } = makeApp(driver)
    const tree = await app.snapshotTree()
    expect(tree.identifier).toBe('root')
    expect((await app.tree()).identifier).toBe('root')
  })

  it('systemPopups returns the wire SystemPopup list', async () => {
    const driver = new MockNodeDriver()
    driver.popupsJson = '[{"id":"p1","type":"alert","source":"SpringBoard","title":"","body":"","buttons":[]}]'
    const { app } = makeApp(driver)
    const popups = await app.systemPopups()
    expect(popups).toHaveLength(1)
    expect(popups[0]?.id).toBe('p1')
    expect(popups[0]?.type).toBe('alert')
  })
})

describe('App act verbs (C3)', () => {
  it('tap resolves the selector to an id and taps it', async () => {
    const { app, driver, resolver } = makeApp()
    resolver.registerHit(encodeSelectorJson(Selector.id('btn-ok')), 'btn-ok')
    await app.tap(Selector.id('btn-ok'))
    expect(driver.calls.find((c) => c.verb === 'tapById')?.args[0]).toBe('btn-ok')
  })

  it('tap throws ELEMENT_NOT_FOUND with visibleElements when nothing resolves', async () => {
    const { app } = makeApp()
    await expect(app.tap(Selector.id('missing'))).rejects.toSatisfy(
      (e: unknown) =>
        e instanceof ExpectationFailure &&
        e.code === 'ELEMENT_NOT_FOUND' &&
        e.visibleElements.length > 0,
    )
  })

  it('fill focuses then types', async () => {
    const { app, driver, resolver } = makeApp()
    resolver.registerHit(encodeSelectorJson(Selector.id('inp')), 'inp')
    await app.fill(Selector.id('inp'), 'hello')
    const verbs = driver.calls.map((c) => c.verb)
    expect(verbs).toEqual(expect.arrayContaining(['tapById', 'inputText']))
    expect(driver.calls.find((c) => c.verb === 'inputText')?.args[0]).toBe('hello')
  })

  it('pressKey maps enter to return and passes others through', async () => {
    const { app, driver } = makeApp()
    await app.pressKey('enter')
    await app.pressKey('escape')
    const keys = driver.calls.filter((c) => c.verb === 'pressKey').map((c) => c.args[0])
    expect(keys).toEqual(['return', 'escape'])
  })

  it('swipe passes the direction through', async () => {
    const { app, driver } = makeApp()
    await app.swipe('up')
    expect(driver.calls.find((c) => c.verb === 'swipe')?.args[0]).toBe('up')
  })

  it('tapAtCoord range-checks then taps', async () => {
    const { app, driver } = makeApp()
    await expect(app.tapAtCoord(1.5, 0.5)).rejects.toSatisfy(
      (e: unknown) => e instanceof ExpectationFailure && e.code === 'ASSERTION_FAILED',
    )
    await app.tapAtCoord(0.5, 0.5)
    expect(driver.calls.find((c) => c.verb === 'tapAtCoord')?.args).toEqual([0.5, 0.5])
  })

  it('terminate and relaunch fire on the session', async () => {
    const { app, driver } = makeApp()
    await app.terminate()
    await app.relaunch()
    const verbs = driver.session.calls.map((c) => c.verb)
    expect(verbs).toEqual(['terminateApp', 'relaunchApp'])
  })
})

describe('Smix.launchApp (C3 entry)', () => {
  it('opens a session, launches, and returns a wired App', async () => {
    const driver = new MockNodeDriver()
    const resolver = new MockSelectorResolver()
    const app = await Smix.launchApp(bundleId('com.acme.app'), driver, resolver.resolve)
    expect(app).toBeInstanceOf(App)
    expect(driver.calls.find((c) => c.verb === 'openSession')?.args[0]).toBe('com.acme.app')
    expect(driver.session.calls.filter((c) => c.verb === 'launchApp')).toHaveLength(1)
  })
})
