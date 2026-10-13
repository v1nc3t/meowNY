import { createContext, useContext } from 'react'
import { t } from './translate'

type Value = { language: string; setLanguage: (language: string) => void }

export const I18nContext = createContext<Value>({ language: 'en', setLanguage: () => undefined })

/** The current language and a way to change it. Using this makes a component redraw when the language changes. */
export function useI18n(): Value & { t: typeof t } {
  return { ...useContext(I18nContext), t }
}

/** Just the translate function; the component still redraws when the language changes. */
export function useT(): typeof t {
  useContext(I18nContext)
  return t
}
