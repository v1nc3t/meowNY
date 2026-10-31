export type TextSize = 'small' | 'default' | 'large' | 'larger'

const STORAGE_KEY = 'ui-text-size'

/** How much each size scales the root font size, and with it everything in rem. */
const SCALE: Record<TextSize, number> = { small: 0.875, default: 1, large: 1.125, larger: 1.25 }

export const TEXT_SIZES = Object.keys(SCALE) as TextSize[]

/** Anything that is not a known size means the default size. */
export function parseTextSize(value: string | null): TextSize {
  return TEXT_SIZES.find((size) => size === value) ?? 'default'
}

export function savedTextSize(): TextSize {
  try {
    return parseTextSize(localStorage.getItem(STORAGE_KEY))
  } catch {
    // Storage can be unavailable; use the default size.
    return 'default'
  }
}

export function applyTextSize(size: TextSize): void {
  document.documentElement.style.setProperty('--text-scale', String(SCALE[size]))
}

export function saveTextSize(size: TextSize): void {
  applyTextSize(size)
  try {
    localStorage.setItem(STORAGE_KEY, size)
  } catch {
    // Not saved; the default size is used next time.
  }
}
