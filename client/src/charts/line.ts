import { BrailleCanvas } from './braille'
import type { Rows, Tone } from './types'

export type LineSeries = {
  /** One value per point, evenly spaced. `null` leaves a gap. */
  values: (number | null)[]
  tone?: Tone
}

export type LineOptions = {
  /** Size in characters. */
  width: number
  height: number
  /** Fixed scale; by default the smallest value (or 0) to the largest. */
  min?: number
  max?: number
}

/**
 * Lines on a braille canvas, like btop's graphs: each series is joined point to point.
 * All series share one scale. The first point sits at the left edge, the last at the right.
 */
export function lineChart(series: LineSeries[], { width, height, min, max }: LineOptions): Rows {
  const canvas = new BrailleCanvas(width, height)
  const all = series.flatMap((s) => s.values).filter((v): v is number => v !== null && Number.isFinite(v))
  const low = min ?? Math.min(0, ...all)
  const high = max ?? Math.max(...all, low)
  const span = high - low || 1
  const top = canvas.dotsHigh - 1
  const yOf = (value: number) => top - ((Math.min(high, Math.max(low, value)) - low) / span) * top

  series.forEach((line, index) => {
    const tone = line.tone ?? index
    const last = line.values.length - 1
    const xOf = (i: number) => (last === 0 ? 0 : (i / last) * (canvas.dotsWide - 1))
    let previous: { x: number; y: number } | null = null
    line.values.forEach((value, i) => {
      if (value === null || !Number.isFinite(value)) {
        previous = null
        return
      }
      const point = { x: xOf(i), y: yOf(value) }
      if (previous) canvas.line(previous.x, previous.y, point.x, point.y, tone)
      else canvas.set(point.x, point.y, tone)
      previous = point
    })
  })
  return canvas.rows()
}
