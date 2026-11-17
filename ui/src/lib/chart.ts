import { useEffect, useRef, useState } from 'react'

// What the hand-drawn charts share: the year chart of the dashboard and the balance chart of an account.

/** The room around the plot: the amounts on the left, the months under it. */
export const PAD = { left: 40, right: 8, top: 10, bottom: 22 }

/** The width of an element in pixels, kept up to date as it resizes. */
export function useWidth() {
  const ref = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(0)
  useEffect(() => {
    const element = ref.current
    if (!element) return
    const observer = new ResizeObserver(([entry]) => setWidth(Math.floor(entry.contentRect.width)))
    observer.observe(element)
    return () => observer.disconnect()
  }, [])
  return [ref, width] as const
}

/** A short number such as 12K, for the axis. */
export function compact(value: number, locale: string): string {
  try {
    return new Intl.NumberFormat(locale, { notation: 'compact', maximumFractionDigits: 1 }).format(value)
  } catch {
    return String(Math.round(value))
  }
}
