import { describe, it, expect } from 'vitest'
import { RunnerSupervisor } from '../runner-supervisor.js'

describe('RunnerSupervisor', () => {
  it('triggers restart on every-50-cases boundary', () => {
    const s = new RunnerSupervisor({ restartEveryNCases: 50, restartAfterNPingFails: 3 })
    for (let i = 0; i < 49; i++) s.recordCase()
    expect(s.shouldRestart()).toBeNull()
    s.recordCase() // 50th
    expect(s.shouldRestart()).toBe('every-50-cases')
  })

  it('triggers restart on 3 consecutive ping failures and resets on success', () => {
    const s = new RunnerSupervisor({ restartEveryNCases: 50, restartAfterNPingFails: 3 })
    s.recordPing(false)
    s.recordPing(false)
    expect(s.shouldRestart()).toBeNull()
    s.recordPing(false)
    expect(s.shouldRestart()).toBe('health-ping-fail-3x')
    s.recordPing(true)
    expect(s.shouldRestart()).toBeNull()
  })

  it('records restarts monotonically and preserves case count', () => {
    const s = new RunnerSupervisor({ restartEveryNCases: 50, restartAfterNPingFails: 3 })
    s.recordCase(); s.recordCase(); s.recordCase()
    s.recordPing(false); s.recordPing(false); s.recordPing(false)
    s.recordRestart()
    expect(s.restarts()).toBe(1)
    expect(s.cases()).toBe(3)
    expect(s.shouldRestart()).toBeNull() // ping counter reset
    s.recordRestart()
    expect(s.restarts()).toBe(2)
    expect(s.cases()).toBe(3)
  })
})
