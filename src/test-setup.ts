// Unit tests mock devices with arbitrary UDIDs (e.g. 'AAA') that do not
// match the .simx/dev-sim.txt lock; disable the lock for vitest runs so
// existing test fixtures keep working. The lock still fires in real CLI/SDK
// invocations where SIMX_DEV_LOCK is unset.
if (process.env.SIMX_DEV_LOCK === undefined) {
  process.env.SIMX_DEV_LOCK = 'off'
}
