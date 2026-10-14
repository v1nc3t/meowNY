import { merge, type Rows, type Seg, type Tone } from './types'

const EIGHTHS = ['', '▏', '▎', '▍', '▌', '▋', '▊', '▉']
const COLUMN = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█']

const clamp01 = (n: number) => (Number.isFinite(n) ? Math.min(1, Math.max(0, n)) : 0)

/**
 * A horizontal meter `width` characters wide, filled to `fraction` (0 to 1) in eighths of a
 * character, the rest drawn dim. Over 1 is shown as full: the caller picks a different tone.
 */
export function meter(fraction: number, width: number, tone: Tone = 0): Seg[] {
  const eighths = Math.round(clamp01(fraction) * width * 8)
  const full = Math.floor(eighths / 8)
  const part = EIGHTHS[eighths % 8]
  const filled = '█'.repeat(full) + part
  const rest = '░'.repeat(Math.max(0, width - full - (part ? 1 : 0)))
  return merge([
    ...(filled ? [{ text: filled, tone }] : []),
    ...(rest ? [{ text: rest, tone: null as Tone }] : []),
  ])
}

/** One character per value, from ▁ to █. The scale runs from 0 to the largest value unless `max` is given. */
export function sparkline(values: number[], tone: Tone = 0, max?: number): Seg[] {
  const top = max ?? Math.max(0, ...values)
  const text = values
    .map((v) => (top > 0 && v > 0 ? COLUMN[Math.max(1, Math.round(clamp01(v / top) * 8))] : COLUMN[0]))
    .join('')
  return [{ text, tone }]
}

export type ColumnGroup = {
  /** One value per series, drawn side by side. */
  values: number[]
}

/**
 * Vertical columns, `height` rows tall, one group per label (a month, say) with one column per
 * series, and a blank column between groups. Every column shares one scale from 0 to the largest value.
 * Series use tones 0, 1, 2 ...; a column of 0 shows nothing.
 */
export function columns(groups: ColumnGroup[], height: number, max?: number): Rows {
  const top = max ?? Math.max(0, ...groups.flatMap((g) => g.values))
  const rows: Rows = []
  for (let row = 0; row < height; row++) {
    const cells: Seg[] = []
    groups.forEach((group, g) => {
      if (g > 0) cells.push({ text: ' ', tone: null })
      group.values.forEach((value, tone) => {
        // How many eighths of this row the column fills: the column's total height minus the rows below.
        const total = top > 0 ? clamp01(value / top) * height * 8 : 0
        const below = (height - 1 - row) * 8
        const level = Math.min(8, Math.max(0, Math.round(total - below)))
        cells.push({ text: COLUMN[level], tone })
      })
    })
    rows.push(merge(cells))
  }
  return rows
}
