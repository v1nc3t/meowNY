import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import { I18nContext } from './context'
import { LANGUAGES, pickLanguage } from './languages'
import { setActiveLanguage } from './translate'

const STORAGE_KEY = 'ui-language'

function isSupported(language: string | null): language is string {
  return language !== null && LANGUAGES.some((known) => known.code === language)
}

/**
 * The language to start in, before the saved settings are known (first run, lock screen):
 * the last one used on this computer, otherwise what the system prefers.
 */
function initialLanguage(): string {
  try {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (isSupported(saved)) return saved
  } catch {
    // Storage can be unavailable; fall through to the system language.
  }
  return pickLanguage(typeof navigator === 'undefined' ? [] : navigator.languages ?? [navigator.language])
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const [language, setLanguageState] = useState(initialLanguage)
  // Set before the children render, so everything below reads the right language.
  setActiveLanguage(language)

  useEffect(() => {
    document.documentElement.lang = language
    try {
      localStorage.setItem(STORAGE_KEY, language)
    } catch {
      // Not saved; the system language is used next time.
    }
  }, [language])

  const setLanguage = useCallback((next: string) => {
    if (isSupported(next)) setLanguageState(next)
  }, [])
  const value = useMemo(() => ({ language, setLanguage }), [language, setLanguage])
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>
}
