/**
 * The pixel art of the app: the logo (the coin cat of scripts/logo.py, served as /favicon.svg) and a dozing cat for a
 * list with nothing in it yet. Both are drawn on a pixel grid, so they are shown at whole multiples of the grid.
 */

/** The logo. `size` in CSS pixels, a multiple of 16 so every pixel stays square. */
export function Logo({ size = 32, className }: { size?: number; className?: string }) {
  return <img src="/favicon.svg" width={size} height={size} alt="" className={className ? `pixel-logo ${className}` : 'pixel-logo'} />
}

// K is the text colour (muted where it is used), Y the gold of the logo.
const DOZING_CAT = [
  '................YYYY',
  '...................Y',
  'K.....K...........Y.',
  'KK...KK..........Y..',
  'KKKKKKK.........YYYY',
  'KK.K.KKKKKKKKK......',
  'KKKKKKKKKKKKKKKK..K.',
  'KKKKKKKKKKKKKKKKK.K.',
  '.KKKKKKKKKKKKKKKKK..',
  '..KKKKKKKKKKKKKKKK..',
  '.KKKKKKKKKKKKKKKKKK.',
]

const FILLS: Record<string, string> = { K: 'currentColor', Y: 'var(--gold)' }

/** One path per colour, each pixel run along a row as a rectangle. */
function paths(rows: readonly string[]) {
  const by: Record<string, string> = {}
  rows.forEach((row, y) => {
    let x = 0
    while (x < row.length) {
      const c = row[x]
      if (c === '.') {
        x++
        continue
      }
      const start = x
      while (x < row.length && row[x] === c) x++
      by[c] = (by[c] ?? '') + `M${start} ${y}h${x - start}v1h-${x - start}z`
    }
  })
  return Object.entries(by).map(([c, d]) => <path key={c} fill={FILLS[c]} d={d} />)
}

const CAT_PATHS = paths(DOZING_CAT)

/** A list with nothing in it: the dozing cat over the hint. */
export function EmptyState({ text }: { text: string }) {
  const w = DOZING_CAT[0].length
  const h = DOZING_CAT.length
  return (
    <div className="empty-state">
      <svg viewBox={`0 0 ${w} ${h}`} width={w * 4} height={h * 4} shapeRendering="crispEdges" aria-hidden="true">
        {CAT_PATHS}
      </svg>
      <p className="hint">{text}</p>
    </div>
  )
}
