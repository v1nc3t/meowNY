export type Theme = 'system' | 'light' | 'dark'

const STORAGE_KEY = 'ui-theme'

export const THEMES: readonly Theme[] = ['system', 'light', 'dark']

/** Anything that is not a known theme means "follow the system". */
export function parseTheme(value: string | null): Theme {
  return value === 'light' || value === 'dark' ? value : 'system'
}

export function savedTheme(): Theme {
  try {
    return parseTheme(localStorage.getItem(STORAGE_KEY))
  } catch {
    // Storage can be unavailable; follow the system.
    return 'system'
  }
}

/**
 * A chosen theme is an attribute on the page. With "system" there is no attribute,
 * and the stylesheet follows the operating system by itself, also while the app is open.
 */
export function applyTheme(theme: Theme): void {
  if (theme === 'system') delete document.documentElement.dataset.theme
  else document.documentElement.dataset.theme = theme
}

export function saveTheme(theme: Theme): void {
  applyTheme(theme)
  try {
    localStorage.setItem(STORAGE_KEY, theme)
  } catch {
    // Not saved; the system theme is used next time.
  }
}
