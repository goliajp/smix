/**
 * Which prebuilt package carries the binaries for the machine running this.
 *
 * smix is a Rust workspace, and `cargo install smix-cli` compiles 27 crates
 * and needs a Rust toolchain — a wall in front of someone whose app is
 * Swift or Kotlin and whose machine has never had rustup on it. This
 * package exists so the binaries arrive prebuilt instead.
 */

/** A target CI builds binaries for, and the npm package they ship in. */
export interface Target {
  /** Rust target triple, as passed to `cargo build --target`. */
  readonly triple: string
  /** `process.platform` this triple serves. */
  readonly platform: string
  /** `process.arch` this triple serves. */
  readonly arch: string
  /** npm package suffix — the part after `@goliapkg/smix-cli-`. */
  readonly suffix: string
}

/**
 * The build matrix, in one place.
 *
 * `.github/workflows/ci.yml`'s `cli-prebuild` job builds exactly these
 * triples, and a test asserts this table against that list. Two copies of a
 * platform list drift, and the drift shows up as a package that resolves to
 * a subpackage nobody published.
 */
export const TARGETS: readonly Target[] = [
  {
    triple: 'aarch64-apple-darwin',
    platform: 'darwin',
    arch: 'arm64',
    suffix: 'darwin-arm64',
  },
  {
    triple: 'x86_64-apple-darwin',
    platform: 'darwin',
    arch: 'x64',
    suffix: 'darwin-x64',
  },
  {
    triple: 'x86_64-unknown-linux-gnu',
    platform: 'linux',
    arch: 'x64',
    suffix: 'linux-x64-gnu',
  },
]

/** What to tell someone whose machine we have no binary for. */
const FALLBACK = 'build from source instead: cargo install smix-cli --locked'

/**
 * The platform package for `platform`/`arch`, or throw saying why not.
 *
 * Never guesses. A binary for the wrong architecture installs cleanly and
 * then fails at exec with a loader error that names neither smix nor the
 * mismatch, which is a worse place to find out than here.
 */
export function resolvePackage(
  platform: string,
  arch: string,
  libc: string | null,
): string {
  if (platform === 'linux' && libc !== null && libc !== 'glibc') {
    throw new Error(
      `smix has no prebuilt binary for linux/${arch} with ${libc} — the published ` +
        `Linux build links against glibc and will not load. ${FALLBACK}`,
    )
  }
  const hit = TARGETS.find((t) => t.platform === platform && t.arch === arch)
  if (hit === undefined) {
    throw new Error(
      `smix has no prebuilt binary for ${platform}/${arch}. Built targets: ` +
        `${TARGETS.map((t) => `${t.platform}/${t.arch}`).join(', ')}. ${FALLBACK}`,
    )
  }
  return `@goliapkg/smix-cli-${hit.suffix}`
}

/** Specifier for one executable inside a platform package. */
export function binaryPath(pkg: string, exe: 'smix' | 'smix-mcp'): string {
  return `${pkg}/${exe}`
}
