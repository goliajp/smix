import { describe, expect, it } from 'vitest'
import { TARGETS, binaryPath, resolvePackage } from '../resolve.js'

describe('resolvePackage', () => {
  it('names the package for each target smix is built for', () => {
    expect(resolvePackage('darwin', 'arm64', null)).toBe('@goliapkg/smix-cli-darwin-arm64')
    expect(resolvePackage('darwin', 'x64', null)).toBe('@goliapkg/smix-cli-darwin-x64')
    expect(resolvePackage('linux', 'x64', 'glibc')).toBe('@goliapkg/smix-cli-linux-x64-gnu')
  })

  it('refuses an unbuilt platform by name, and says what is left to try', () => {
    // Returning something for a platform we do not build would install a
    // binary that cannot run. Saying nothing at all leaves someone with a
    // missing command and no next move, so the refusal carries the source
    // build that still works everywhere.
    let message = ''
    try {
      resolvePackage('win32', 'arm64', null)
      throw new Error('resolvePackage should have refused win32/arm64')
    } catch (e) {
      message = (e as Error).message
    }
    expect(message).toContain('win32')
    expect(message).toContain('arm64')
    expect(message).toContain('cargo install smix-cli')
  })

  it('refuses musl rather than handing it a glibc build', () => {
    // A glibc binary on musl fails at exec with a loader error that says
    // nothing about smix. Refuse where the cause is still legible.
    expect(() => resolvePackage('linux', 'x64', 'musl')).toThrow(/musl/)
  })

  it('the target table is the one CI builds', () => {
    expect(TARGETS.map((t) => t.triple).sort()).toEqual([
      'aarch64-apple-darwin',
      'x86_64-apple-darwin',
      'x86_64-unknown-linux-gnu',
    ])
  })
})

describe('binaryPath', () => {
  it('points at the executable inside the platform package', () => {
    expect(binaryPath('@goliapkg/smix-cli-darwin-arm64', 'smix')).toBe(
      '@goliapkg/smix-cli-darwin-arm64/smix',
    )
    expect(binaryPath('@goliapkg/smix-cli-linux-x64-gnu', 'smix-mcp')).toBe(
      '@goliapkg/smix-cli-linux-x64-gnu/smix-mcp',
    )
  })
})
