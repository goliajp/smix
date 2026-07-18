import { useEffect, useRef, useState } from 'react'

// The signature. smix's true animation-idle samples the screen and asks
// "is it still?" instead of sleeping a fixed 400ms (commit "answer 'is the
// screen still?' instead of sleeping 400ms"; docs/v2.md C3 decision log).
//
// The grid is 12×8 = 96 cells — the real default sample grid is 96 points.
// The readout numbers (Δpx 4, max-moved 4) are the shipped defaults from the
// same decision log. On load the grid shows motion, then settles to "still"
// after two quiet samples, mirroring the mechanism. Reduced motion pins it
// to the settled state.

const COLS = 12
const ROWS = 8
const CELLS = COLS * ROWS // 96 — matches the real grid.

const MOTION_MS = 1250
const SETTLE_HOLD_MS = 1900
const TICK_MS = 90

type Phase = 'motion' | 'still'

function prefersReducedMotion(): boolean {
  try {
    return window.matchMedia('(prefers-reduced-motion: reduce)').matches
  } catch {
    return false
  }
}

// A few fixed cells stay lit in the "still" frame so the grid reads as an
// active sampler at rest rather than a dead panel.
const STILL_ANCHORS = new Set([27, 40, 52, 68])

export function IdleSampler() {
  const [phase, setPhase] = useState<Phase>('still')
  const [activity, setActivity] = useState<number[]>(() => new Array<number>(CELLS).fill(0))
  const timers = useRef<number[]>([])

  useEffect(() => {
    if (prefersReducedMotion()) {
      setPhase('still')
      setActivity(new Array<number>(CELLS).fill(0))
      return
    }

    let tickHandle = 0
    const clearAll = () => {
      window.clearInterval(tickHandle)
      timers.current.forEach((t) => window.clearTimeout(t))
      timers.current = []
    }

    const runCycle = () => {
      setPhase('motion')
      tickHandle = window.setInterval(() => {
        setActivity(Array.from({ length: CELLS }, () => (Math.random() < 0.42 ? Math.random() : 0)))
      }, TICK_MS)

      timers.current.push(
        window.setTimeout(() => {
          window.clearInterval(tickHandle)
          setPhase('still')
          setActivity(new Array<number>(CELLS).fill(0))
          timers.current.push(window.setTimeout(runCycle, SETTLE_HOLD_MS))
        }, MOTION_MS),
      )
    }

    const start = window.setTimeout(runCycle, 500)
    timers.current.push(start)
    return clearAll
  }, [])

  return (
    <figure className="m-0 border border-border bg-bg-inset">
      <figcaption className="flex items-center justify-between border-b border-border px-4 py-2">
        <span className="mono-label">animation-idle</span>
        <span
          className="flex items-center gap-2 font-mono text-[11px] tracking-wide uppercase"
          style={{ color: phase === 'motion' ? 'var(--decide)' : 'var(--sense)' }}
        >
          <span
            aria-hidden
            className="inline-block h-2 w-2"
            style={{ backgroundColor: phase === 'motion' ? 'var(--decide)' : 'var(--sense)' }}
          />
          {phase === 'motion' ? 'sampling' : 'still — 2 frames'}
        </span>
      </figcaption>

      <div
        className="grid gap-[3px] p-4"
        style={{ gridTemplateColumns: `repeat(${COLS}, 1fr)` }}
        role="img"
        aria-label={
          phase === 'motion'
            ? 'Frame-diff sampler detecting motion on the screen.'
            : 'Frame-diff sampler reporting the screen is still.'
        }
      >
        {activity.map((a, i) => {
          const lit = phase === 'still' ? STILL_ANCHORS.has(i) : a > 0.12
          const isAnchor = phase === 'still' && STILL_ANCHORS.has(i)
          return (
            <span
              key={i}
              className="aspect-square"
              style={{
                backgroundColor: lit
                  ? isAnchor
                    ? 'var(--sense)'
                    : 'var(--decide)'
                  : 'var(--grid)',
                opacity: phase === 'motion' && lit ? 0.35 + a * 0.65 : 1,
                transition: phase === 'still' ? 'background-color 220ms ease, opacity 220ms ease' : 'none',
              }}
            />
          )
        })}
      </div>

      <div className="flex flex-wrap gap-x-5 gap-y-1 border-t border-border px-4 py-2 font-mono text-[11px] text-fg-muted">
        <span>frame-diff</span>
        <span>grid 96</span>
        <span>Δpx 4</span>
        <span>max-moved 4</span>
        <span className="text-fg-dim">no fixed sleep</span>
      </div>
    </figure>
  )
}
