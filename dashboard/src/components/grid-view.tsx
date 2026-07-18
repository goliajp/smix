import { type JSX } from 'react'

import type { SimEntry } from '../views/live'
import { GridCell } from './grid-cell'

// Reflow rules: N=2 → 2-col strip;  N∈[3,4] → 2×2;  N∈[5,9] → 3×3.
// N=1 is handled upstream — LiveView forces single mode there so the
// grid never has to render a degenerate single cell. Literal class names
// keep Tailwind's static scanner happy.
function gridColsClass(n: number): string {
  if (n <= 2) return 'grid-cols-2'
  if (n <= 4) return 'grid-cols-2'
  return 'grid-cols-3'
}

// The NOC wall. Outer top + left rules; each cell wrapper draws its own
// right + bottom — together the rules tile into a clean 1px grid with no
// double-borders and no gaps (brutalist density). auto-rows-fr keeps every
// row equal height so the wall reads as a single composed surface.
export function GridView({
  sims,
  selectedUdid,
  onCellClick,
}: {
  sims: SimEntry[]
  selectedUdid: string | null
  onCellClick: (udid: string) => void
}): JSX.Element {
  return (
    <div
      data-grid-view="true"
      className={
        'grid h-full w-full auto-rows-fr border-t border-l border-[color:var(--border)] bg-[color:var(--bg)] ' +
        gridColsClass(sims.length)
      }
    >
      {sims.map((sim) => (
        <div
          key={sim.udid}
          className="min-h-0 min-w-0 border-r border-b border-[color:var(--border)]"
        >
          <GridCell
            sim={sim}
            active={sim.udid === selectedUdid}
            onClick={() => onCellClick(sim.udid)}
          />
        </div>
      ))}
    </div>
  )
}
