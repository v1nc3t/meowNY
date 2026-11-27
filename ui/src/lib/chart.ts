import { useEffect, useRef, useState } from 'react'

// What the hand-drawn charts share: the year chart of the dashboard and the balance chart of an account.
// A chart is drawn on a grid of 16ths of a rem (one unit is 1px when the root text is 16px) and scaled to the text size by its viewBox, so
// the numbers in a chart (the room around the plot, the heights, the size of the text) are units of that grid, not pixels of the screen.

/** The room around the plot, in chart units: the amounts on the left, the months under it. */
export const PAD = { left: 40, right: 8, top: 10, bottom: 22 }

/** The size of the text of the axes, in chart units (0.75rem). */
export const AXIS_FONT = 12

/** The width of an element in pixels, kept up to date as it resizes, and how many pixels one chart unit is (the root text size / 16). */
export function useWidth() {
  const ref = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(0)
  const [unit, setUnit] = useState(1)
  useEffect(() => {
    const element = ref.current
    if (!element) return
    const observer = new ResizeObserver(([entry]) => {
      setWidth(Math.floor(entry.contentRect.width))
      setUnit((parseFloat(getComputedStyle(document.documentElement).fontSize) || 16) / 16)
    })
    observer.observe(element)
    return () => observer.disconnect()
  }, [])
  return [ref, width, unit] as const
}

/** The size of an element in pixels, kept up to date as it resizes, and how many pixels one chart unit is. For a chart that fills the room it is given. */
export function useSize() {
  const ref = useRef<HTMLDivElement>(null)
  const [size, setSize] = useState({ width: 0, height: 0, unit: 1 })
  useEffect(() => {
    const element = ref.current
    if (!element) return
    const observer = new ResizeObserver(([entry]) => {
      const unit = (parseFloat(getComputedStyle(document.documentElement).fontSize) || 16) / 16
      setSize({ width: Math.floor(entry.contentRect.width), height: Math.floor(entry.contentRect.height), unit })
    })
    observer.observe(element)
    return () => observer.disconnect()
  }, [])
  return [ref, size] as const
}

/** A short number such as 12K, for the axis. */
export function compact(value: number, locale: string): string {
  try {
    return new Intl.NumberFormat(locale, { notation: 'compact', maximumFractionDigits: 1 }).format(value)
  } catch {
    return String(Math.round(value))
  }
}
