// v0.7 C1 — long-run stability supervisor.
// Pure decision logic: caller drives side-effects (spawn/kill xcodebuild,
// curl /health). This class only counts and decides — keeps unit-testable.

export type RestartReason = 'every-50-cases' | 'health-ping-fail-3x'

export interface RunnerSupervisorOptions {
  restartEveryNCases: number
  restartAfterNPingFails: number
}

export class RunnerSupervisor {
  private readonly opts: RunnerSupervisorOptions
  private caseCount = 0
  private consecutivePingFails = 0
  private restartCount = 0

  constructor(opts: RunnerSupervisorOptions) {
    if (opts.restartEveryNCases <= 0) throw new Error('restartEveryNCases must be > 0')
    if (opts.restartAfterNPingFails <= 0) throw new Error('restartAfterNPingFails must be > 0')
    this.opts = opts
  }

  recordCase(): void {
    this.caseCount += 1
  }

  recordPing(ok: boolean): void {
    if (ok) this.consecutivePingFails = 0
    else this.consecutivePingFails += 1
  }

  shouldRestart(): RestartReason | null {
    if (this.consecutivePingFails >= this.opts.restartAfterNPingFails) {
      return 'health-ping-fail-3x'
    }
    if (this.caseCount > 0 && this.caseCount % this.opts.restartEveryNCases === 0) {
      // Only fire on the boundary case; recordRestart() clears ping-fails so
      // back-to-back checks on the same boundary do not double-fire across the
      // caller loop (caller calls shouldRestart() once per case, restarts, then
      // moves on to the next case which bumps caseCount past the boundary).
      return 'every-50-cases'
    }
    return null
  }

  recordRestart(): void {
    this.restartCount += 1
    this.consecutivePingFails = 0
  }

  restarts(): number { return this.restartCount }
  cases(): number { return this.caseCount }
  pingFails(): number { return this.consecutivePingFails }
}
