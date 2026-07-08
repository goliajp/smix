import { useCallback, useEffect, useMemo, useRef, useState, type JSX, type ReactNode } from 'react'

import { GridView } from '../components/grid-view'
import { ModeSwitch, type ViewMode } from '../components/mode-switch'
import { useHls } from '../hooks/use-hls'
import { getSims, startCapture, stopCapture } from '../lib/api'

// /live — multi-sim live-capture observation panel. NOC wall: sharp corners,
// 1px warm-tan rules, mono-uppercase metadata, accent pulse on LIVE markers.
// web-v0.3 c4 introduced the chrome (left sim rail + right metadata sidebar
// + dual-theme tokens) on a single-sim well; web-v0.4 c3 keeps the chrome
// and swaps the center main into a grid mode (N path monitors share 1px
// ruled edges, no gaps) ↔ single-detail mode (the v0.3 well, untouched).
//
// Default mode = grid when N>=2, single when N<=1. A grid cell click drops
// into single mode focused on that sim. The mode-switch lets the operator
// flip back without losing the selection.

export type SimEntry = {
  udid: string
  device_name: string
  runtime: string
  stream_path: string
  started_at: string // RFC3339
  capturing: boolean
}

function shortUdid(udid: string): string {
  const head = udid.split('-')[0] ?? udid
  return head.length >= 8 ? head : udid
}

function relTime(iso: string): string {
  const t = Date.parse(iso)
  if (Number.isNaN(t)) return iso
  const secs = Math.max(0, Math.floor((Date.now() - t) / 1000))
  if (secs < 60) return `${secs}s`
  const mins = Math.floor(secs / 60)
  if (mins < 60) return `${mins}m`
  const hrs = Math.floor(mins / 60)
  if (hrs < 24) return `${hrs}h`
  return `${Math.floor(hrs / 24)}d`
}

function StatusDot({ live }: { live: boolean }): JSX.Element {
  return (
    <span
      aria-hidden
      className={
        'inline-block h-1.5 w-1.5 ' +
        (live ? 'animate-pulse bg-[color:var(--accent)]' : 'bg-[color:var(--fg-dim)]')
      }
    />
  )
}

function RailLabel({ children }: { children: ReactNode }): JSX.Element {
  return (
    <div className="flex items-center gap-2 border-b border-[color:var(--border)] bg-[color:var(--bg-elev)] px-4 py-2.5 font-mono text-[11px] tracking-[0.16em] uppercase">
      {children}
    </div>
  )
}

function CornerTicks(): JSX.Element {
  const base = 'absolute h-3 w-3 border-[color:var(--border-strong)]'
  return (
    <>
      <span aria-hidden className={base + ' top-0 left-0 border-t border-l'} />
      <span aria-hidden className={base + ' top-0 right-0 border-t border-r'} />
      <span aria-hidden className={base + ' bottom-0 left-0 border-b border-l'} />
      <span aria-hidden className={base + ' right-0 bottom-0 border-r border-b'} />
    </>
  )
}

export function LiveView({ sims: injected }: { sims?: SimEntry[] }): JSX.Element {
  // Injected sims (tests) take priority and disable polling.
  const injectedMode = injected != null
  const [fetched, setFetched] = useState<SimEntry[]>(injected ?? [])
  const [selectedUdid, setSelectedUdid] = useState<string | null>(injected?.[0]?.udid ?? null)
  const [error, setError] = useState<string | null>(null)
  // null = "let N decide" (grid when N>=2, single when N<=1). User toggling
  // mode pins the override; cell click also pins → single.
  const [modeOverride, setModeOverride] = useState<ViewMode | null>(null)
  const videoRef = useRef<HTMLVideoElement>(null)

  const sims = injectedMode ? injected : fetched

  useEffect(() => {
    if (injectedMode) return
    let alive = true
    const poll = async (): Promise<void> => {
      try {
        const next = await getSims()
        if (!alive) return
        setFetched(next)
        setError(null)
      } catch (e) {
        if (!alive) return
        setError(e instanceof Error ? e.message : String(e))
      }
    }
    void poll()
    const id = setInterval(() => void poll(), 2500)
    return () => {
      alive = false
      clearInterval(id)
    }
  }, [injectedMode])

  const refresh = useCallback(async (): Promise<void> => {
    try {
      const next = await getSims()
      setFetched(next)
      setError(null)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  const selected = useMemo(
    () => sims.find((s) => s.udid === selectedUdid) ?? sims[0] ?? null,
    [sims, selectedUdid]
  )

  // N<=1 always renders single (grid of 1 has no point); otherwise the
  // override wins, with grid as the N>=2 default.
  const effectiveMode: ViewMode = sims.length <= 1 ? 'single' : (modeOverride ?? 'grid')

  // Single-mode hls is only wired up when we're actually showing the well.
  // In grid mode each <GridCell> spins up its own useHls instance.
  const singleEnabled = effectiveMode === 'single' && (selected?.capturing ?? false)
  const streamSrc = singleEnabled && selected ? `/streams/${selected.udid}/index.m3u8` : null
  useHls(videoRef, streamSrc, singleEnabled)

  const onStart = useCallback(
    async (udid: string): Promise<void> => {
      try {
        await startCapture(udid)
        await refresh()
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e))
      }
    },
    [refresh]
  )
  const onStop = useCallback(
    async (udid: string): Promise<void> => {
      try {
        await stopCapture(udid)
        await refresh()
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e))
      }
    },
    [refresh]
  )

  const onCellClick = useCallback((udid: string) => {
    setSelectedUdid(udid)
    setModeOverride('single')
  }, [])

  const onModeChange = useCallback((m: ViewMode) => {
    setModeOverride(m)
  }, [])

  return (
    <div className="flex h-full w-full bg-[color:var(--bg)] text-[color:var(--fg)]">
      {/* ── LEFT RAIL: sim list ── */}
      <aside className="flex w-[264px] shrink-0 flex-col border-r border-[color:var(--border)]">
        <RailLabel>
          <span className="text-[color:var(--accent)]">sims</span>
          <span className="text-[color:var(--fg-dim)]">·</span>
          <span className="text-[color:var(--fg-muted)]">{sims.length}</span>
        </RailLabel>
        <div className="flex-1 overflow-y-auto">
          {sims.length === 0 ? (
            <div className="px-4 py-6 font-mono text-[11px] leading-relaxed tracking-[0.08em] text-[color:var(--fg-muted)] uppercase">
              no sims booted
              <div className="mt-1 text-[color:var(--fg-dim)] normal-case">
                boot a sim and start capture to view its live screen here.
              </div>
            </div>
          ) : (
            sims.map((sim) => {
              const active = sim.udid === selected?.udid
              return (
                <button
                  key={sim.udid}
                  type="button"
                  title={sim.udid}
                  onClick={() => setSelectedUdid(sim.udid)}
                  className={
                    'block w-full border-b border-[color:var(--border)] px-4 py-2.5 text-left ' +
                    (active
                      ? 'border-l-2 border-l-[color:var(--accent)] bg-[color:var(--bg-inset)]'
                      : 'border-l-2 border-l-transparent hover:bg-[color:var(--bg-elev)]')
                  }
                >
                  <div className="flex items-center gap-2 font-mono text-[12px]">
                    <StatusDot live={sim.capturing} />
                    <span
                      className={active ? 'text-[color:var(--accent)]' : 'text-[color:var(--fg)]'}
                    >
                      {shortUdid(sim.udid)}
                    </span>
                  </div>
                  <div className="mt-1 truncate pl-3.5 font-mono text-[10.5px] tracking-[0.06em] text-[color:var(--fg-muted)]">
                    {sim.device_name}
                  </div>
                </button>
              )
            })
          )}
        </div>
      </aside>

      {/* ── CENTER: live well (single) or NOC wall (grid) ── */}
      <main className="flex min-w-0 flex-1 flex-col">
        {/* status bar */}
        <div className="flex items-center justify-between border-b border-[color:var(--border)] bg-[color:var(--bg-elev)] px-5 py-2.5">
          <div className="flex min-w-0 items-center gap-3 font-mono text-[11px] tracking-[0.14em] uppercase">
            <span className="text-[color:var(--accent)]">live</span>
            <span className="text-[color:var(--fg-dim)]">/</span>
            {effectiveMode === 'grid' ? (
              <>
                <span className="text-[color:var(--fg)]">{sims.length} sims</span>
                <span className="text-[color:var(--fg-dim)]">·</span>
                <span className="text-[color:var(--fg-muted)]">grid</span>
              </>
            ) : (
              <span className="truncate text-[color:var(--fg)]" title={selected?.udid}>
                {selected ? selected.udid : '—'}
              </span>
            )}
          </div>
          <div className="flex items-center gap-3">
            {error != null ? (
              <span
                className="max-w-[280px] truncate font-mono text-[10.5px] tracking-[0.08em] text-[color:var(--accent)] uppercase"
                title={error}
              >
                err · {error}
              </span>
            ) : null}
            {effectiveMode === 'single' && selected ? (
              <span
                className={
                  'chip ' + (selected.capturing ? 'chip-accent' : 'text-[color:var(--fg-muted)]')
                }
              >
                <StatusDot live={selected.capturing} />
                {selected.capturing ? 'capturing' : 'idle'}
              </span>
            ) : null}
            {sims.length >= 2 ? (
              <ModeSwitch mode={effectiveMode} onModeChange={onModeChange} />
            ) : null}
          </div>
        </div>

        {/* center body — branch on mode */}
        {effectiveMode === 'grid' ? (
          <GridView sims={sims} selectedUdid={selected?.udid ?? null} onCellClick={onCellClick} />
        ) : (
          <>
            {/* video well (single mode) — v0.3 c4 form, untouched */}
            <div className="flex min-h-0 flex-1 items-center justify-center bg-[color:var(--bg-inset)] p-6">
              {selected == null ? (
                <div className="font-mono text-[12px] tracking-[0.1em] text-[color:var(--fg-muted)] uppercase">
                  no sim selected
                </div>
              ) : (
                <div
                  className="relative h-full"
                  style={{ aspectRatio: '1206 / 2622', maxWidth: '100%' }}
                >
                  <CornerTicks />
                  <video
                    ref={videoRef}
                    muted
                    playsInline
                    className="h-full w-full bg-black"
                    data-udid={selected.udid}
                  />
                  {!selected.capturing ? (
                    <div className="absolute inset-0 flex items-center justify-center bg-[color:var(--bg-inset)] px-6 text-center font-mono text-[11px] leading-relaxed tracking-[0.08em] text-[color:var(--fg-muted)] uppercase">
                      not capturing
                      <br />— start to view
                    </div>
                  ) : null}
                </div>
              )}
            </div>

            {/* controls (single mode only) */}
            <div className="flex items-center gap-3 border-t border-[color:var(--border)] bg-[color:var(--bg-elev)] px-5 py-3.5">
              <button
                type="button"
                disabled={selected == null || selected.capturing}
                onClick={() => {
                  if (selected) void onStart(selected.udid)
                }}
                className="border border-[color:var(--accent)] bg-[color:var(--accent)] px-4 py-2 font-mono text-[12px] tracking-[0.16em] text-[color:var(--accent-fg)] uppercase no-underline hover:opacity-90 disabled:cursor-not-allowed disabled:border-[color:var(--border)] disabled:bg-transparent disabled:text-[color:var(--fg-dim)] disabled:opacity-100"
              >
                start capture →
              </button>
              <button
                type="button"
                disabled={selected == null || !selected.capturing}
                onClick={() => {
                  if (selected) void onStop(selected.udid)
                }}
                className="border border-[color:var(--border-strong)] bg-[color:var(--bg)] px-4 py-2 font-mono text-[12px] tracking-[0.16em] text-[color:var(--fg)] uppercase hover:border-[color:var(--accent)] hover:text-[color:var(--accent)] disabled:cursor-not-allowed disabled:border-[color:var(--border)] disabled:text-[color:var(--fg-dim)]"
              >
                stop
              </button>
            </div>
          </>
        )}
      </main>

      {/* ── RIGHT: metadata sidebar ── */}
      <aside className="flex w-[312px] shrink-0 flex-col border-l border-[color:var(--border)]">
        <RailLabel>
          <span className="text-[color:var(--accent)]">metadata</span>
        </RailLabel>
        <div className="flex-1 overflow-y-auto px-5 py-4">
          {selected == null ? (
            <div className="font-mono text-[11px] tracking-[0.08em] text-[color:var(--fg-muted)] uppercase">
              —
            </div>
          ) : (
            <dl className="space-y-3 font-mono text-[12px]">
              <MetaRow label="udid">
                <span className="break-all text-[color:var(--fg)] select-all">{selected.udid}</span>
              </MetaRow>
              <MetaRow label="device">{selected.device_name}</MetaRow>
              <MetaRow label="runtime">{selected.runtime}</MetaRow>
              <MetaRow label="capturing">
                <span
                  className={
                    'chip ' + (selected.capturing ? 'chip-accent' : 'text-[color:var(--fg-muted)]')
                  }
                >
                  {selected.capturing ? 'true' : 'false'}
                </span>
              </MetaRow>
              <MetaRow label="started">
                {relTime(selected.started_at)}{' '}
                <span className="text-[color:var(--fg-dim)]">ago</span>
              </MetaRow>
              <MetaRow label="stream">
                <span className="break-all text-[color:var(--fg-muted)]">
                  {selected.stream_path}
                </span>
              </MetaRow>
            </dl>
          )}
        </div>
      </aside>
    </div>
  )
}

function MetaRow({ label, children }: { label: string; children: ReactNode }): JSX.Element {
  return (
    <div className="flex items-baseline gap-3">
      <dt className="w-20 shrink-0 text-[color:var(--fg-dim)] uppercase">{label}</dt>
      <dd className="min-w-0 flex-1">{children}</dd>
    </div>
  )
}
