import { useRef, type JSX } from 'react'

import { useHls } from '../hooks/use-hls'
import type { SimEntry } from '../views/live'

function shortUdid(udid: string): string {
  // First block of a UDID is plenty to disambiguate in the dense grid footer;
  // full value stays available via title= and the metadata sidebar.
  const head = udid.split('-')[0] ?? udid
  return head.length >= 8 ? head : udid
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

// One monitor on the NOC wall. The wrapper is the click target so a tap
// anywhere on the cell — video, overlay, or the footer strip — switches the
// LiveView to single-sim detail focused on this sim. Each cell owns its
// own <video> ref and useHls instance; on unmount the hook's effect tears
// down hls.js (loader + MSE buffers + timers), so cycling cells in/out of
// the grid does not leak across switches.
export function GridCell({
  sim,
  active,
  onClick,
}: {
  sim: SimEntry
  active: boolean
  onClick: () => void
}): JSX.Element {
  const videoRef = useRef<HTMLVideoElement>(null)
  const src = sim.capturing ? `/streams/${sim.udid}/index.m3u8` : null
  useHls(videoRef, src, sim.capturing)

  return (
    <button
      type="button"
      onClick={onClick}
      data-grid-cell="true"
      aria-label={`${sim.udid} · ${sim.device_name}`}
      title={sim.udid}
      className={
        'group relative flex h-full w-full flex-col bg-[color:var(--bg-inset)] p-0 text-left ' +
        (active
          ? 'outline outline-1 -outline-offset-1 outline-[color:var(--accent)]'
          : 'hover:outline hover:outline-1 hover:-outline-offset-1 hover:outline-[color:var(--accent)]')
      }
    >
      {/* TL live indicator — decorative, does not block clicks (button is
          the receiver, video etc. bubble up). */}
      <div className="pointer-events-none absolute top-2 left-2 z-10 flex items-center gap-1.5 font-mono text-[9.5px] tracking-[0.18em] uppercase">
        <StatusDot live={sim.capturing} />
        <span
          className={sim.capturing ? 'text-[color:var(--accent)]' : 'text-[color:var(--fg-dim)]'}
        >
          {sim.capturing ? 'live' : 'idle'}
        </span>
      </div>

      {/* Video well — keep iOS 9:19.5 portrait. Wrapper handles aspect ratio
          so the video letterboxes inside the cell on any column width. */}
      <div className="flex min-h-0 flex-1 items-center justify-center bg-black p-3">
        <div className="relative h-full" style={{ aspectRatio: '1206 / 2622', maxWidth: '100%' }}>
          <video
            ref={videoRef}
            muted
            playsInline
            data-udid={sim.udid}
            className="h-full w-full bg-black"
          />
          {!sim.capturing ? (
            <div className="absolute inset-0 flex items-center justify-center bg-[color:var(--bg-inset)] px-3 text-center font-mono text-[10px] leading-relaxed tracking-[0.1em] text-[color:var(--fg-muted)] uppercase">
              not capturing
              <br />— start to view
            </div>
          ) : null}
        </div>
      </div>

      {/* Footer — broadcast-equipment label strip. */}
      <div className="flex items-center justify-between gap-2 border-t border-[color:var(--border)] bg-[color:var(--bg-elev)] px-3 py-2 font-mono text-[10.5px] tracking-[0.14em] uppercase">
        <span className="flex min-w-0 items-center gap-2 truncate">
          <span className={active ? 'text-[color:var(--accent)]' : 'text-[color:var(--fg)]'}>
            {shortUdid(sim.udid)}
          </span>
          <span className="text-[color:var(--fg-dim)]">·</span>
          <span className="truncate text-[color:var(--fg-muted)]">{sim.device_name}</span>
        </span>
        {sim.capturing ? (
          <span className="shrink-0 text-[10px] tracking-[0.18em] text-[color:var(--accent)]">
            ◉ live
          </span>
        ) : (
          <span className="shrink-0 text-[10px] tracking-[0.18em] text-[color:var(--fg-dim)]">
            ○ idle
          </span>
        )}
      </div>
    </button>
  )
}
