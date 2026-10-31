const STORAGE_KEY = 'ui-text-size'

/** The text size is a percentage of the normal size. It scales the root font size, and with it everything in rem. */
export const TEXT_SIZE = { min: 80, max: 150, step: 5, normal: 100 } as const

/** A saved size is kept within the range; anything that is not a number is the normal size. */
export function parseTextSize(value: string | null): number {
  const size = Number.parseInt(value ?? '', 10)
  return Number.isNaN(size) ? TEXT_SIZE.normal : Math.min(TEXT_SIZE.max, Math.max(TEXT_SIZE.min, size))
}

export function savedTextSize(): number {
  try {
    return parseTextSize(localStorage.getItem(STORAGE_KEY))
  } catch {
    // Storage can be unavailable; use the normal size.
    return TEXT_SIZE.normal
  }
}

export function applyTextSize(size: number): void {
  document.documentElement.style.setProperty('--text-scale', String(size / 100))
}

export function saveTextSize(size: number): void {
  applyTextSize(size)
  try {
    localStorage.setItem(STORAGE_KEY, String(size))
  } catch {
    // Not saved; the normal size is used next time.
  }
}
