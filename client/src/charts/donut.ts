import { BrailleCanvas } from './braille'
import { merge, type Rows, type Tone } from './types'

export type Slice = { value: number; tone?: Tone }

export type DonutOptions = {
  /** Height in characters; the width is twice that, because a character is half as wide as it is tall. */
  size: number
  /** Text in the middle, cut to fit the hole. */
  center?: string
  /** Inner radius as a share of the outer one. Default 0.6. */
  hole?: number
}

/**
 * A ring on a braille canvas, slices clockwise from twelve o'clock. Negative and non-finite
 * values count as zero. With nothing to show the ring is drawn dim.
 */
export function donut(slices: Slice[], { size, center, hole = 0.6 }: DonutOptions): Rows {
  const canvas = new BrailleCanvas(size * 2, size)
  const values = slices.map((s) => (Number.isFinite(s.value) && s.value > 0 ? s.value : 0))
  const total = values.reduce((a, b) => a + b, 0)
  const ends = values.map((_, i) => values.slice(0, i + 1).reduce((a, b) => a + b, 0) / (total || 1))

  const radius = (canvas.dotsHigh - 1) / 2
  const inner = radius * hole
  const cx = (canvas.dotsWide - 1) / 2
  const cy = (canvas.dotsHigh - 1) / 2
  for (let y = 0; y < canvas.dotsHigh; y++) {
    for (let x = 0; x < canvas.dotsWide; x++) {
      const dx = x - cx
      const dy = y - cy
      const distance = Math.hypot(dx, dy)
      if (distance > radius || distance < inner) continue
      if (total === 0) {
        canvas.set(x, y, null)
        continue
      }
      // 0 at twelve o'clock, growing clockwise to 1.
      const turn = (Math.atan2(dx, -dy) + 2 * Math.PI) % (2 * Math.PI) / (2 * Math.PI)
      const index = ends.findIndex((end, i) => values[i] > 0 && turn < end)
      const at = index === -1 ? values.findLastIndex((v) => v > 0) : index
      canvas.set(x, y, slices[at].tone ?? at)
    }
  }

  const rows = canvas.rows()
  if (center) {
    const room = Math.max(0, Math.floor(inner)) // characters across the hole: 2 * inner dots / 2 dots per character
    const text = [...center].slice(0, room).join('')
    const row = Math.floor(size / 2)
    const left = Math.floor((canvas.width - text.length) / 2)
    const cells = Array.from({ length: canvas.width }, (_, col) =>
      col >= left && col < left + text.length ? { text: text[col - left], tone: null as Tone } : canvas.cell(col, row),
    )
    rows[row] = merge(cells)
  }
  return rows
}
