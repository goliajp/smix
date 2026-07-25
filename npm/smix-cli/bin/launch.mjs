// Shared launcher for the two executables this package installs.
//
// Both are ordinary native binaries: the npm package is a delivery
// mechanism, not a wrapper. So this hands over argv untouched, inherits
// all three streams, and exits with whatever the binary exited with.
//
// Inheriting stdio is not a detail — `smix-mcp` speaks MCP over stdin and
// stdout, and anything that buffers, re-encodes, or writes a line of its
// own into those streams corrupts the protocol.
import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { dirname } from 'node:path'
import process from 'node:process'

import { binaryPath, resolvePackage } from '../dist/resolve.js'

/** glibc vs musl, or null when the runtime will not say. */
function detectLibc() {
  // Node reports the libc family in process.report on Linux. On other
  // platforms the question does not arise, and a wrong guess here would
  // refuse an install that was fine.
  if (process.platform !== 'linux') return null
  try {
    const report = process.report?.getReport()
    const header = typeof report === 'object' && report !== null ? report.header : undefined
    const glibc = header?.glibcVersionRuntime
    return typeof glibc === 'string' && glibc !== '' ? 'glibc' : 'musl'
  } catch {
    return null
  }
}

/**
 * Run `exe` with this process's arguments and exit with its status.
 *
 * @param {'smix' | 'smix-mcp'} exe
 */
export function launch(exe) {
  const pkg = resolvePackage(process.platform, process.arch, detectLibc())

  let binary
  try {
    // Resolve the package's manifest rather than the binary itself:
    // resolution of an extensionless file is not something a package
    // without an `exports` map is obliged to support, and the manifest
    // always resolves.
    const require = createRequire(import.meta.url)
    binary = binaryPath(dirname(require.resolve(`${pkg}/package.json`)), exe)
  } catch {
    // An optional dependency that did not install is the ordinary way this
    // fails, and `--no-optional` or an offline install is how it happens.
    process.stderr.write(
      `smix: ${pkg} is not installed, so there is no ${exe} binary for this machine.\n` +
        `Reinstall without --no-optional, or build from source: cargo install smix-cli --locked\n`,
    )
    process.exit(127)
  }

  const result = spawnSync(binary, process.argv.slice(2), { stdio: 'inherit' })
  if (result.error !== undefined) {
    process.stderr.write(`smix: could not run ${binary}: ${result.error.message}\n`)
    process.exit(126)
  }
  // A binary killed by a signal has no exit code. Reporting 0 there would
  // tell a script the run succeeded.
  if (result.status === null) {
    process.stderr.write(`smix: ${exe} was killed by ${result.signal}\n`)
    process.exit(128)
  }
  process.exit(result.status)
}
