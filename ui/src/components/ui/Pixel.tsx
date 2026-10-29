import Button from './Button'

/**
 * The pixel art of the app: the logo (the coin cat of scripts/logo.py, served as /favicon.svg), a dozing cat for a list with nothing
 * in it yet, and the small pictures of what the app does (first start, first steps). All are drawn on a pixel grid, so they are
 * shown at whole multiples of the grid.
 */

/** The logo. `size` in CSS pixels, a multiple of 16 so every pixel stays square. */
export function Logo({ size = 32, className }: { size?: number; className?: string }) {
  return <img src="/favicon.svg" width={size} height={size} alt="" className={className ? `pixel-logo ${className}` : 'pixel-logo'} />
}

// K is the text colour (muted where it is used), Y the gold of the logo, G the green of the app.
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

const FILLS: Record<string, string> = { K: 'currentColor', Y: 'var(--gold)', G: 'var(--primary)' }

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

/** A list with nothing in it: the dozing cat over the hint, and a button for the first thing to do when there is one. */
export function EmptyState({ text, action }: { text: string; action?: { label: string; onClick: () => void } }) {
  const w = DOZING_CAT[0].length
  const h = DOZING_CAT.length
  return (
    <div className="empty-state">
      <svg viewBox={`0 0 ${w} ${h}`} width={w * 4} height={h * 4} shapeRendering="crispEdges" aria-hidden="true">
        {CAT_PATHS}
      </svg>
      <p className="hint">{text}</p>
      {action ? <Button onClick={action.onClick}>{action.label}</Button> : null}
    </div>
  )
}

// What the app does, one 16x16 picture each: a receipt with a coin (spending), a jar that fills (budgets), two cats and a coin
// (bills shared with friends), a padlock (private, on this device) and a laptop with a phone (the two kept in step).
const PICTURES = {
  spend: [
    '................',
    '.KKKKKKKKKK.....',
    '.K........K.....',
    '.K.KKKKKK.K.....',
    '.K........K.....',
    '.K.KKKK...K.....',
    '.K........K.....',
    '.K.KKKKK..K.....',
    '.K........KKKK..',
    '.K.KKK...KYYYYK.',
    '.K......KYYYYYYK',
    '.K......KYYKKYYK',
    '.K.K.K.KKYYKKYYK',
    '..K.K.K.KYYYYYYK',
    '.........KYYYYK.',
    '..........KKKK..',
  ],
  budget: [
    '......YYYY......',
    '.....YYYYYY.....',
    '.....YYYYYY.....',
    '......YYYY......',
    '................',
    '...KKKK..KKKK...',
    '...KKKKKKKKKK...',
    '....K......K....',
    '...K........K...',
    '...K........K...',
    '...KGGGGGGGGK...',
    '...KGGGGGGGGK...',
    '...KGGGGGGGGK...',
    '...KGGGGGGGGK...',
    '...KKKKKKKKKK...',
    '................',
  ],
  share: [
    '................',
    '................',
    '......YYYY......',
    '.....YYYYYY.....',
    '.....YYYYYY.....',
    '......YYYY......',
    '................',
    'K....K....G....G',
    'KK..KK....GG..GG',
    'KKKKKK....GGGGGG',
    'K.KK.K....G.GG.G',
    'KKKKKK....GGGGGG',
    'KKKKKK....GGGGGG',
    '.KKKK......GGGG.',
    '................',
    '................',
  ],
  lock: [
    '................',
    '................',
    '.....KKKKKK.....',
    '....KK....KK....',
    '....K......K....',
    '....K......K....',
    '....K......K....',
    '..KKKKKKKKKKKK..',
    '..KYYYYYYYYYYK..',
    '..KYYYYYYYYYYK..',
    '..KYYYYKKYYYYK..',
    '..KYYYYKKYYYYK..',
    '..KYYYYKKYYYYK..',
    '..KYYYYYYYYYYK..',
    '..KKKKKKKKKKKK..',
    '................',
  ],
  sync: [
    '................',
    '................',
    '................',
    '................',
    '...........KKKKK',
    '.KKKKKKKK..KGGGK',
    '.K......K..KGGGK',
    '.K.GGGG.K..KGGGK',
    '.K......K..KGGGK',
    '.K.GG...K..KGGGK',
    '.K......K..KGGGK',
    '.KKKKKKKK..KGGGK',
    'KKKKKKKKKK.KKKKK',
    '................',
    '................',
    '................',
  ],
} as const

export type PictureName = keyof typeof PICTURES

const PICTURE_PATHS = Object.fromEntries(Object.entries(PICTURES).map(([name, rows]) => [name, paths(rows)])) as Record<
  PictureName,
  ReturnType<typeof paths>
>

/** One of the pictures of what the app does. `scale` is how many CSS pixels one pixel of the grid takes (a whole number). */
export function Illustration({ name, scale = 3, className }: { name: PictureName; scale?: number; className?: string }) {
  return (
    <svg
      className={className ? `pixel-art ${className}` : 'pixel-art'}
      viewBox="0 0 16 16"
      width={16 * scale}
      height={16 * scale}
      shapeRendering="crispEdges"
      aria-hidden="true"
    >
      {PICTURE_PATHS[name]}
    </svg>
  )
}
