#!/usr/bin/env bun
// v0.7 C1 — 100-case long-run stability smoke.
// Boots the SimxRunner XCUITest server, drives 100 serial launch+terminate
// cycles on Settings (com.apple.Preferences), uses RunnerSupervisor to
// restart the runner every 50 cases or after 3 consecutive /health ping
// failures. Emits one JSON line to stdout:
//   {total, passed, failed, restarts, restart_reasons[], app_bundle_id,
//    dev_sim_udid, elapsed_ms}
//
// Constraints (CLAUDE.md §9 unchanged + v0.7 C1 task):
//   - dev sim is read from .simx/dev-sim.txt; no env fallback, no
//     first-booted fallback (DevSimLockViolation = hard fail signal).
//   - sim is NOT rebooted across the 100 cases — only the XCUITest runner
//     process restarts.
//   - 0 new deps. Reuses RunnerClient + dev-lock + RunnerSupervisor.

import { spawn, spawnSync, type ChildProcess } from 'node:child_process'
import { mkdirSync, openSync } from 'node:fs'
import { join } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'

import { readDevSimLock, assertDevSimLock } from '../src/sim/dev-lock.js'
import { RunnerClient } from '../src/sim/runner-client.js'
import { RunnerSupervisor, type RestartReason } from '../src/sim/runner-supervisor.js'

const ROOT = process.cwd()
const PORT = 22087
const BUNDLE_ID = 'com.apple.Preferences'
const TOTAL_CASES = 100
const HEALTH_TIMEOUT_MS = 120_000
const HEALTH_POLL_INTERVAL_MS = 1_000

type RestartEvent = { at_case: number; reason: RestartReason }

function resolveLockedUdid(): string {
  const udid = readDevSimLock(ROOT)
  if (udid === null) {
    console.error('[long-run-100] .simx/dev-sim.txt missing; dev-sim lock required')
    process.exit(2)
  }
  assertDevSimLock(udid, ROOT)
  return udid
}

function startRunner(udid: string): { child: ChildProcess; logFile: string } {
  const logDir = join(ROOT, '.simx/runner')
  mkdirSync(logDir, { recursive: true })
  const logFile = join(logDir, `longrun-${process.pid}-${Date.now()}.log`)
  const project = join(ROOT, 'swift-bridge/SimxRunner.xcodeproj')
  const args = [
    '-project', project,
    '-scheme', 'SimxRunner',
    '-destination', `platform=iOS Simulator,id=${udid}`,
    '-only-testing:SimxRunnerUITests/SimxRunnerUITests/test_runForever',
    'test',
  ]
  const out = openSync(logFile, 'a')
  const child = spawn('xcodebuild', args, { stdio: ['ignore', out, out], detached: false })
  return { child, logFile }
}

async function waitHealth(client: RunnerClient, timeoutMs: number): Promise<boolean> {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    if (await client.health()) return true
    await sleep(HEALTH_POLL_INTERVAL_MS)
  }
  return false
}

async function stopRunner(child: ChildProcess): Promise<void> {
  const pid = child.pid
  if (pid === undefined) return
  // xcodebuild spawns descendants (testmanagerd lane); pkill -P to reap them,
  // then SIGTERM the leader. Wait briefly so the next xcodebuild test doesn't
  // race the old one on the same port.
  try { spawnSync('pkill', ['-P', String(pid)]) } catch { /* ignore */ }
  try { child.kill('SIGTERM') } catch { /* already dead */ }
  // Give the OS a moment to release :22087 before the next startRunner.
  await sleep(2000)
}

function runCase(udid: string, bundleId: string): boolean {
  const launch = spawnSync('xcrun', ['simctl', 'launch', udid, bundleId], { stdio: 'ignore' })
  if (launch.status !== 0) return false
  spawnSync('sleep', ['1'])
  const term = spawnSync('xcrun', ['simctl', 'terminate', udid, bundleId], { stdio: 'ignore' })
  return term.status === 0
}

async function main(): Promise<void> {
  const udid = resolveLockedUdid()
  const start = Date.now()
  let runner = startRunner(udid).child
  const client = new RunnerClient({ port: PORT })

  if (!(await waitHealth(client, HEALTH_TIMEOUT_MS))) {
    await stopRunner(runner)
    console.error('[long-run-100] runner did not become healthy within timeout')
    process.exit(1)
  }

  const supervisor = new RunnerSupervisor({ restartEveryNCases: 50, restartAfterNPingFails: 3 })
  const restartReasons: RestartEvent[] = []
  let passed = 0
  let failed = 0

  try {
    for (let i = 1; i <= TOTAL_CASES; i++) {
      const ok = runCase(udid, BUNDLE_ID)
      if (ok) passed += 1
      else failed += 1
      supervisor.recordCase()

      const pingOk = await client.health()
      supervisor.recordPing(pingOk)

      const reason = supervisor.shouldRestart()
      if (reason !== null) {
        await stopRunner(runner)
        runner = startRunner(udid).child
        if (!(await waitHealth(client, HEALTH_TIMEOUT_MS))) {
          await stopRunner(runner)
          console.error(`[long-run-100] runner failed to recover after restart at case=${i}`)
          process.exit(1)
        }
        supervisor.recordRestart()
        restartReasons.push({ at_case: i, reason })
      }
    }
  } finally {
    await stopRunner(runner)
  }

  const out = {
    total: TOTAL_CASES,
    passed,
    failed,
    restarts: supervisor.restarts(),
    restart_reasons: restartReasons,
    app_bundle_id: BUNDLE_ID,
    dev_sim_udid: udid,
    elapsed_ms: Date.now() - start,
  }
  console.log(JSON.stringify(out))
  process.exit(passed === TOTAL_CASES && failed === 0 ? 0 : 1)
}

main().catch((e) => {
  console.error(`[long-run-100] fatal: ${(e as Error).message}`)
  process.exit(2)
})
